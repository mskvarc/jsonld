use jsonld_core::{ExpandedDocument, FlattenedDocument, Loader};
use jsonld_syntax::{IntoJson, Keyword};
use rdf_rs::vocabulary::{self, Vocabulary};
use std::hash::Hash;

use crate::{CompactFragment, iri::IriConfusedWithPrefix};

pub type CompactDocumentResult = Result<jstrict::Value, crate::Error>;

/// Context embeding method.
///
/// This trait provides the `embed_context` method that can be used
/// to include a JSON-LD context to a JSON-LD document.
/// It is used at the end of compaction algorithm to embed to
/// context used to compact the document into the compacted output.
pub trait EmbedContext {
    /// Embeds the given context into the document.
    fn embed_context<N>(
        &mut self,
        vocabulary: &N,
        context: jsonld_context_processing::ProcessedRef<N::Iri, N::BlankId>,
        options: crate::Options,
    ) -> Result<(), IriConfusedWithPrefix>
    where
        N: Vocabulary + jsonld_core::ParallelSafeVocabulary,
        N::Iri: Clone + Hash + Eq,
        N::BlankId: Clone + Hash + Eq;
}

/// Compaction function.
pub trait Compact<I, B> {
    /// Compacts the input document with full options.
    #[allow(async_fn_in_trait)]
    async fn compact_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
    ) -> CompactDocumentResult
    where
        N: rdf_rs::vocabulary::VocabularyMut<Iri = I, BlankId = B> + jsonld_core::ParallelSafeVocabulary,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader;

    /// Compacts the input document with the given `vocabulary` to
    /// interpret identifiers.
    #[allow(async_fn_in_trait)]
    async fn compact_with<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
    ) -> CompactDocumentResult
    where
        N: rdf_rs::vocabulary::VocabularyMut<Iri = I, BlankId = B> + jsonld_core::ParallelSafeVocabulary,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.compact_full(vocabulary, context, loader, crate::Options::default()).await
    }

    /// Compacts the input document.
    #[allow(async_fn_in_trait)]
    async fn compact<'a, L>(&'a self, context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>, loader: &'a L) -> CompactDocumentResult
    where
        (): rdf_rs::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
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
    ) -> CompactDocumentResult
    where
        N: rdf_rs::vocabulary::VocabularyMut<Iri = I, BlankId = B> + jsonld_core::ParallelSafeVocabulary,
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
    ) -> CompactDocumentResult
    where
        N: rdf_rs::vocabulary::VocabularyMut<Iri = I, BlankId = B> + jsonld_core::ParallelSafeVocabulary,
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
        N: Vocabulary + jsonld_core::ParallelSafeVocabulary,
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
