use jsonld_context_processing::WarningHandler;
use jsonld_core::{ExpandedDocument, FlattenedDocument, Loader, warning};
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
    ///
    /// # Errors
    ///
    /// Returns an error when the context cannot be embedded in the compacted document.
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
    ///
    /// `warnings` receives every warning raised while processing the scoped
    /// contexts compaction meets.
    async fn compact_full<'a, N, L, W>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
        warnings: W,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
        W: WarningHandler<N>;

    /// Compacts this document with the default
    /// [`Options`][crate::Options], using `vocabulary` to resolve IRI and blank
    /// node identifiers.
    ///
    /// Warnings are printed to standard error.
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
        self.compact_full(vocabulary, context, loader, crate::Options::default(), warning::Print).await
    }

    /// Compacts this document with the default [`Options`][crate::Options], for
    /// documents that store IRIs and blank node identifiers inline instead of
    /// indexing them through a vocabulary.
    ///
    /// Warnings are printed to standard error.
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
    async fn compact_full<'a, N, L, W>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
        mut warnings: W,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
        W: WarningHandler<N>,
    {
        let mut compacted_output = self
            .objects()
            .compact_fragment_full(vocabulary, context.processed(), context.processed(), None, loader, options, &mut warnings)
            .await?;

        compacted_output.embed_context(vocabulary, context, options)?;

        Ok(compacted_output)
    }
}

impl<I, B> Compact<I, B> for FlattenedDocument<I, B> {
    async fn compact_full<'a, N, L, W>(
        &'a self,
        vocabulary: &'a mut N,
        context: jsonld_context_processing::ProcessedRef<'a, 'a, I, B>,
        loader: &'a L,
        options: crate::Options,
        mut warnings: W,
    ) -> CompactDocumentResult<L::Error>
    where
        N: rdfx::vocabulary::VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
        W: WarningHandler<N>,
    {
        let mut compacted_output = self
            .compact_fragment_full(vocabulary, context.processed(), context.processed(), None, loader, options, &mut warnings)
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

            *self = obj.into();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::{Compact, Options, test_fixtures::json};
    use jsonld_context_processing::{Process, ProcessedRef, Warning};
    use jsonld_core::{ExpandedDocument, NoLoader, TryFromJson, warning::WarningBuf};
    use jsonld_syntax::{TryFromJson as _, context::Context as SyntaxContext};
    use rdfx::{BlankIdBuf, IriBuf, vocabulary::no_vocabulary_mut};

    /// A property-scoped context is processed again when compaction reaches a
    /// node written under that property, and what that processing reports
    /// reaches the handler given to `compact_full`.
    #[tokio::test]
    async fn scoped_context_warnings_reach_the_compaction_handler() {
        let context = SyntaxContext::try_from_json(&json(
            r#"{
                "p": { "@id": "https://example.com/p", "@context": { "bad": { "@id": "@foo" } } },
                "q": "https://example.com/q"
            }"#,
        ))
        .unwrap();
        let mut processing_warnings = WarningBuf::<Warning>::new();
        let processed = context
            .process_full(
                no_vocabulary_mut(),
                &jsonld_core::Context::new(None),
                &NoLoader,
                None,
                jsonld_context_processing::Options::default(),
                &mut processing_warnings,
            )
            .await
            .unwrap()
            .into_processed();
        let document = ExpandedDocument::<IriBuf, BlankIdBuf>::try_from_json_in(
            no_vocabulary_mut(),
            json(r#"[{ "https://example.com/p": [{ "https://example.com/q": [{ "@value": 1 }] }] }]"#),
        )
        .unwrap();

        let mut warnings = WarningBuf::<Warning>::new();
        document
            .compact_full(
                no_vocabulary_mut(),
                ProcessedRef::new(&context, &processed),
                &NoLoader,
                Options::default(),
                &mut warnings,
            )
            .await
            .unwrap();

        assert_eq!(warnings.0, vec![Warning::KeywordLikeValue(String::from("@foo"))]);
    }
}
