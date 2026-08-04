use jsonld_core::{ExpandedDocument, FlattenedDocument, Loader};
use jsonld_syntax::{IntoJson, Keyword};
use rdfx::vocabulary::{self, Vocabulary};
use std::hash::Hash;

use crate::{CompactFragment, iri::IriConfusedWithPrefix};

/// Result of compacting a whole document.
pub type CompactDocumentResult<E> = Result<jstrict::Value, crate::Error<E>>;

/// Documents that a `@context` can be embedded into.
///
/// The final step of document compaction is to record, in the output, the
/// context the output was compacted against, so that the result round-trips
/// back to the same expanded document.
pub trait EmbedContext {
    /// Embeds `context` into this document as a `@context` entry.
    ///
    /// The context is written in its original, unprocessed form, as the first
    /// entry of the document's top-level object. A document that compacted to an
    /// array is first wrapped in an object under whatever `@graph` compacts to,
    /// since only an object can carry `@context`. Nothing is embedded when the
    /// document compacted to `null` or to nothing at all, or when the context
    /// itself is null or empty.
    fn embed_context<N>(
        &mut self,
        vocabulary: &N,
        context: jsonld_context_processing::ProcessedRef<N::Iri, N::BlankId>,
        options: crate::Options,
    ) -> Result<(), IriConfusedWithPrefix>
    where
        N: Vocabulary,
        N::Iri: Clone + Hash + Eq,
        N::BlankId: Clone + Hash + Eq;
}

/// Whole documents that can be compacted against a processed `@context`.
///
/// Implemented for [`ExpandedDocument`] and [`FlattenedDocument`]. Unlike
/// [`CompactFragment`], these methods take the context in both its processed and
/// its original form: the processed form drives compaction, and the original form
/// is embedded into the output as its `@context`.
pub trait Compact<I, B> {
    /// Compacts this document with the given [`Options`][crate::Options].
    async fn compact_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader;

    /// Compacts this document with the default
    /// [`Options`][crate::Options], using `vocabulary` to resolve IRI and blank
    /// node identifiers.
    async fn compact_with<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.compact_full(vocabulary, context, loader, crate::Options::default()).await
    }

    /// Compacts this document with the default [`Options`][crate::Options], for
    /// documents that store IRIs and blank node identifiers inline instead of
    /// indexing them through a vocabulary.
    async fn compact<'a, L>(&'a self, context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>, loader: &'a L) -> CompactDocumentResult<L::Error>
    where
        (): rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.compact_with(vocabulary::no_vocabulary_mut(), context, loader).await
    }
}

impl<I, B> Compact<I, B> for ExpandedDocument<I, B> {
    async fn compact_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        let mut compacted_output = self
            .objects()
            .compact_fragment_full(vocabulary, context.processed(), context.processed(), None, loader, options)
            .await?;

        compacted_output.embed_context(vocabulary, context, options)?;

        Ok(compacted_output)
    }
}

impl<I, B> Compact<I, B> for FlattenedDocument<I, B> {
    async fn compact_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        let mut compacted_output = self
            .compact_fragment_full(vocabulary, context.processed(), context.processed(), None, loader, options)
            .await?;

        compacted_output.embed_context(vocabulary, context, options)?;

        Ok(compacted_output)
    }
}

impl EmbedContext for jstrict::Value {
    fn embed_context<N>(
        &mut self,
        vocabulary: &N,
        context: jsonld_context_processing::ProcessedRef<N::Iri, N::BlankId>,
        options: crate::Options,
    ) -> Result<(), IriConfusedWithPrefix>
    where
        N: Vocabulary,
        N::Iri: Clone + Hash + Eq,
        N::BlankId: Clone + Hash + Eq,
    {
        let value = self.take();

        let obj = match value {
            jstrict::Value::Array(array) => {
                let mut obj = jstrict::Object::new();

                if !array.is_empty() {
                    let key = crate::iri::keyword_alias(vocabulary, context.processed(), options, Keyword::Graph);

                    obj.insert(key.into(), array.into());
                }

                Some(obj)
            }
            jstrict::Value::Object(obj) => Some(obj),
            _null => None,
        };

        if let Some(mut obj) = obj {
            let json_context = IntoJson::into_json(context.unprocessed().clone());

            if !obj.is_empty() && !json_context.is_null() && !json_context.is_empty_array_or_object() {
                obj.insert_front("@context".into(), json_context);
            }

            *self = obj.into()
        };

        Ok(())
    }
}
