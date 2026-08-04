//! Implementation of the [JSON-LD compaction algorithms][1].
//!
//! Compaction turns an expanded JSON-LD document back into the terse,
//! human-friendly shape described by a `@context`: IRIs become terms or
//! compact IRIs, single-element arrays collapse, and value objects reduce to
//! plain JSON scalars wherever the context makes that lossless.
//!
//! [`Compact`] compacts a whole document (an
//! [`ExpandedDocument`][jsonld_core::ExpandedDocument] or a
//! [`FlattenedDocument`][jsonld_core::FlattenedDocument]) and embeds the
//! `@context` used into the output. [`CompactFragment`] compacts a single
//! object, node, list or collection against an already-processed active
//! context, and is what the document-level algorithm recurses through.
//!
//! This crate is the compaction half of the `jsonld` family; the `jsonld`
//! crate re-exports it behind a processor API that also handles expansion,
//! flattening and RDF serialization.
//!
//! [1]: https://www.w3.org/TR/json-ld-api/#compaction-algorithms
use jsonld_context_processing::{Options as ProcessingOptions, Process};
use jsonld_core::{
    Context,
    ContextRef,
    IndexSet,
    Indexed,
    Loader,
    ProcessingMode,
    Term,
    Value,
    context::inverse::{LangSelection, TypeSelection},
    object::Any,
};
use jsonld_syntax::{ContainerKind, ErrorCode, Keyword};
use rdfx::vocabulary::{self, VocabularyMut};
use std::hash::Hash;

mod document;
mod iri;
mod node;
mod property;
mod value;

pub use document::*;
pub use iri::IriConfusedWithPrefix;
pub(crate) use iri::*;
use node::*;
use property::*;
use value::*;

#[derive(Debug, thiserror::Error)]
/// Error raised while compacting a document.
pub enum Error<E = std::convert::Infallible> {
    #[error("IRI confused with prefix")]
    /// An IRI cannot be written out because it would be read back as a
    /// compact IRI.
    ///
    /// See [`IriConfusedWithPrefix`] for the exact condition.
    IriConfusedWithPrefix,

    #[error("Invalid `@nest` value")]
    /// A term definition's `@nest` value is neither `@nest` itself nor a term
    /// of the active context that expands to `@nest`.
    InvalidNestValue,

    #[error("Colliding compacted entry")]
    /// The compaction target of a map-based container or `@nest` entry
    /// already holds a non-map value: two terms compacted to the same key
    /// with incompatible shapes. The JSON-LD specification does not define
    /// an output for this state.
    CollidingEntry,

    #[error("Context processing failed: {0}")]
    /// A `@context` encountered during compaction could not be processed.
    ///
    /// Compaction processes scoped contexts (`@context` entries inside term
    /// definitions) as it descends, so any context-processing error can
    /// surface here.
    ContextProcessing(jsonld_context_processing::Error<E>),
}

impl<E> Error<E> {
    /// Returns the JSON-LD error code this error is reported under.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::IriConfusedWithPrefix => ErrorCode::IriConfusedWithPrefix,
            Self::InvalidNestValue => ErrorCode::InvalidNestValue,
            Self::CollidingEntry => ErrorCode::ConflictingIndexes,
            Self::ContextProcessing(e) => e.code(),
        }
    }
}

impl<E> From<jsonld_context_processing::Error<E>> for Error<E> {
    fn from(e: jsonld_context_processing::Error<E>) -> Self {
        Self::ContextProcessing(e)
    }
}

impl<E> From<IriConfusedWithPrefix> for Error<E> {
    fn from(_: IriConfusedWithPrefix) -> Self {
        Self::IriConfusedWithPrefix
    }
}

/// Result of compacting a document fragment.
pub type CompactFragmentResult<E> = Result<jstrict::Value, Error<E>>;

/// Compaction options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// JSON-LD processing mode.
    pub processing_mode: ProcessingMode,

    /// Whether IRIs may be compacted into references relative to the
    /// document's location.
    ///
    /// The compaction algorithm itself does not read this flag: it relativizes
    /// IRIs whenever the active context has a base IRI. The flag is what
    /// decides whether that base IRI gets set in the first place — the
    /// `jsonld` processor seeds the active context with the input document's
    /// URL only when this is `true` and no explicit base IRI was given.
    pub compact_to_relative: bool,

    /// Whether a single-element array is replaced by that element.
    ///
    /// When `false`, arrays stay arrays even with one element. Terms whose
    /// container mapping includes `@set`, and the `@graph` and `@list` keys,
    /// keep their arrays regardless.
    pub compact_arrays: bool,

    /// Whether the entries of each node object are processed in lexicographic
    /// order of their expanded property IRI.
    ///
    /// Only affects the order of keys in the output, not which keys appear.
    pub ordered: bool,
}

impl From<Options> for jsonld_context_processing::Options {
    fn from(options: Options) -> jsonld_context_processing::Options {
        jsonld_context_processing::Options {
            processing_mode: options.processing_mode,
            ..Default::default()
        }
    }
}

impl From<jsonld_expansion::Options> for Options {
    fn from(options: jsonld_expansion::Options) -> Options {
        Options {
            processing_mode: options.processing_mode,
            ordered: options.ordered,
            ..Options::default()
        }
    }
}

impl Default for Options {
    fn default() -> Options {
        Options {
            processing_mode: ProcessingMode::default(),
            compact_to_relative: true,
            compact_arrays: true,
            ordered: false,
        }
    }
}

/// Document fragments that can be compacted against an active context.
pub trait CompactFragment<I, B> {
    /// Compacts this fragment, taking every parameter of the algorithm
    /// explicitly.
    ///
    /// `active_context` is the context in force for this fragment.
    /// `type_scoped_context` is the context as it stood *before* this
    /// fragment's own type-scoped contexts were applied; term definitions for
    /// `active_property` and for `@type` values are looked up there, as the
    /// specification requires. `active_property` is the term this fragment was
    /// reached through, and drives container, language and index handling.
    /// `loader` resolves any remote context referenced by a scoped `@context`.
    async fn compact_fragment_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader;

    #[inline(always)]
    /// Compacts this fragment at the top level of a document, with the default
    /// [`Options`], using `vocabulary` to resolve IRI and blank node
    /// identifiers.
    async fn compact_fragment_with<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        loader: &'a mut L,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.compact_fragment_full(vocabulary, active_context, active_context, None, loader, Options::default())
            .await
    }

    #[inline(always)]
    /// Compacts this fragment at the top level of a document, with the default
    /// [`Options`], for documents that store IRIs and blank node identifiers
    /// inline instead of indexing them through a vocabulary.
    async fn compact_fragment<'a, L>(&'a self, active_context: &'a Context<I, B>, loader: &'a mut L) -> CompactFragmentResult<L::Error>
    where
        (): VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.compact_fragment_full(
            vocabulary::no_vocabulary_mut(),
            active_context,
            active_context,
            None,
            loader,
            Options::default(),
        )
        .await
    }
}

#[derive(PartialEq)]
enum TypeLangValue<'a, I> {
    Type(TypeSelection<I>),
    Lang(LangSelection<'a>),
}

/// Document fragments that carry an `@index` separately from their own
/// contents.
///
/// [`Indexed<T>`] pairs a fragment with the index it was reached through;
/// implementing this trait for `T` is what gives `Indexed<T>` its
/// [`CompactFragment`] implementation. The index has to be passed down rather
/// than read off the fragment because whether it survives into the output
/// depends on the active property's container mapping.
pub trait CompactIndexedFragment<I, B> {
    /// Compacts this fragment together with the `@index` value it was reached
    /// through.
    ///
    /// The index is emitted as an `@index` entry unless the active property's
    /// container mapping includes `@index`, in which case it has already become
    /// the enclosing map's key and is dropped.
    async fn compact_indexed_fragment<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        index: Option<&'a str>,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader;
}

impl<I, B, T: CompactIndexedFragment<I, B>> CompactFragment<I, B> for Indexed<T> {
    async fn compact_fragment_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        self.inner()
            .compact_indexed_fragment(vocabulary, self.index(), active_context, type_scoped_context, active_property, loader, options)
            .await
    }
}

impl<I, B, T: Any<I, B>> CompactIndexedFragment<I, B> for T {
    async fn compact_indexed_fragment<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        index: Option<&'a str>,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        use jsonld_core::object::Ref;
        match self.as_ref() {
            Ref::Value(value) => compact_indexed_value_with(vocabulary, value, index, active_context, active_property, loader, options).await,
            Ref::Node(node) => compact_indexed_node_with(vocabulary, node, index, active_context, type_scoped_context, active_property, loader, options).await,
            Ref::List(list) => {
                let mut active_context = active_context;
                // If active context has a previous context, the active context is not propagated.
                // If element does not contain an @value entry, and element does not consist of
                // a single @id entry, set active context to previous context from active context,
                // as the scope of a term-scoped context does not apply when processing new node objects.
                if let Some(previous_context) = active_context.previous_context() {
                    active_context = previous_context
                }

                // If the term definition for active property in active context has a local context:
                //
                // Known deviation from the specification text, which says to look the
                // term definition up in the active context. This looks it up in
                // `type_scoped_context` instead, which is what makes the W3C
                // compaction test suite pass; the spec text is believed to be in error.
                // See https://github.com/w3c/json-ld-api/issues/502.
                let mut active_context = ContextRef::Borrowed(active_context);
                let mut list_container = false;
                if let Some(active_property) = active_property
                    && let Some(active_property_definition) = type_scoped_context.get(active_property)
                {
                    if let Some(local_context) = active_property_definition.context() {
                        active_context = ContextRef::owned(
                            local_context
                                .process_with(
                                    vocabulary,
                                    active_context.as_ref(),
                                    loader,
                                    active_property_definition.base_url().cloned(),
                                    ProcessingOptions::from(options).with_override(),
                                )
                                .await?
                                .into_processed(),
                        )
                    }

                    list_container = active_property_definition.container().contains(ContainerKind::List);
                }

                if list_container {
                    compact_collection_with(
                        vocabulary,
                        list.iter(),
                        active_context.as_ref(),
                        active_context.as_ref(),
                        active_property,
                        loader,
                        options,
                    )
                    .await
                } else {
                    let mut result = jstrict::Object::default();
                    compact_property(
                        vocabulary,
                        &mut result,
                        Term::Keyword(Keyword::List),
                        list,
                        active_context.as_ref(),
                        loader,
                        false,
                        options,
                    )
                    .await?;

                    // If expanded property is @index and active property has a container mapping in
                    // active context that includes @index,
                    if let Some(index) = index {
                        let mut index_container = false;
                        if let Some(active_property) = active_property
                            && let Some(active_property_definition) = active_context.get(active_property)
                            && active_property_definition.container().contains(ContainerKind::Index)
                        {
                            // then the compacted result will be inside of an @index container,
                            // drop the @index entry by continuing to the next expanded property.
                            index_container = true;
                        }

                        if !index_container {
                            // Initialize alias by IRI compacting expanded property.
                            let alias = crate::iri::keyword_alias(vocabulary, active_context.as_ref(), options, Keyword::Index);

                            // Add an entry alias to result whose value is set to expanded value and continue with the next expanded property.
                            result.insert(alias.into(), jstrict::Value::String(index.into()));
                        }
                    }

                    Ok(jstrict::Value::Object(result))
                }
            }
        }
    }
}

/// Adds `value` under `key` in `map`, following the JSON-LD API's [add value][1]
/// algorithm.
///
/// A key that already holds something becomes an array of both values. When
/// `as_array` is true the value is wrapped in an array even if `key` was absent,
/// so that a term with an `@set` container always compacts to an array. An
/// array `value` is added element by element rather than nested.
///
/// [1]: https://www.w3.org/TR/json-ld-api/#add-value
fn add_value(map: &mut jstrict::Object, key: &str, value: jstrict::Value, as_array: bool) {
    match value {
        jstrict::Value::Array(values) => {
            // Establish the entry's shape before adding the elements: wrap an
            // existing scalar into a single-element array, or insert an empty
            // array when `as_array` is set and the entry is absent, so that an
            // empty `values` still leaves an array behind.
            // Compaction-built objects never have duplicate keys, so the `Err`
            // (duplicate key) case of `get_unique_mut` is treated as absent.
            match map.get_unique_mut(key).ok().flatten() {
                Some(existing) if !existing.is_array() => {
                    let prev = std::mem::replace(existing, jstrict::Value::Array(Vec::new()));
                    if let jstrict::Value::Array(arr) = existing {
                        arr.push(prev);
                    }
                }
                None if as_array => {
                    map.insert(key.into(), jstrict::Value::Array(Vec::new()));
                }
                _ => {}
            }
            for v in values {
                add_value(map, key, v, false);
            }
        }
        scalar => match map.get_unique_mut(key).ok().flatten() {
            Some(jstrict::Value::Array(arr)) => {
                arr.push(scalar);
            }
            Some(existing) => {
                // Existing scalar — wrap into [prev, new] array.
                let prev = std::mem::replace(existing, jstrict::Value::Array(Vec::new()));
                if let jstrict::Value::Array(arr) = existing {
                    arr.push(prev);
                    arr.push(scalar);
                }
            }
            None if as_array => {
                map.insert(key.into(), jstrict::Value::Array(vec![scalar]));
            }
            None => {
                map.insert(key.into(), scalar);
            }
        },
    }
}

/// Returns the JSON value carried by the `@value` entry of a value object,
/// discarding its `@type`, `@language` and `@direction`.
fn value_value<I>(value: &Value<I>) -> jstrict::Value {
    use jsonld_core::object::Literal;
    match value {
        Value::Literal(lit, _ty) => match lit {
            Literal::Null => jstrict::Value::Null,
            Literal::Boolean(b) => jstrict::Value::Boolean(*b),
            Literal::Number(n) => jstrict::Value::Number(n.clone()),
            Literal::String(s) => jstrict::Value::String(s.as_str().into()),
        },
        Value::LangString(s) => jstrict::Value::String(s.as_str().into()),
        Value::Json(json) => json.clone(),
    }
}

/// Compacts every item of a collection, then unwraps the result to a single
/// value when the `compactArrays` option allows it.
///
/// Items compacting to `null` are dropped. A single-element array is kept as an
/// array when `options.compact_arrays` is `false`, when `active_property` is
/// `@graph` or `@set`, or when its container mapping includes `@list` or
/// `@set`.
async fn compact_collection_with<'a, N, L, O, T>(
    vocabulary: &'a mut N,
    items: O,
    active_context: &'a Context<N::Iri, N::BlankId>,
    type_scoped_context: &'a Context<N::Iri, N::BlankId>,
    active_property: Option<&'a str>,
    loader: &'a L,
    options: Options,
) -> CompactFragmentResult<L::Error>
where
    N: VocabularyMut,
    N::Iri: Clone + Hash + Eq,
    N::BlankId: Clone + Hash + Eq,
    T: 'a + CompactFragment<N::Iri, N::BlankId>,
    O: 'a + Iterator<Item = &'a T>,
    L: Loader,
{
    let mut result = Vec::new();

    for item in items {
        let compacted_item = Box::pin(item.compact_fragment_full(vocabulary, active_context, type_scoped_context, active_property, loader, options)).await?;

        if !compacted_item.is_null() {
            result.push(compacted_item)
        }
    }

    let mut list_or_set = false;
    if let Some(active_property) = active_property
        && let Some(active_property_definition) = active_context.get(active_property)
    {
        list_or_set =
            active_property_definition.container().contains(ContainerKind::List) || active_property_definition.container().contains(ContainerKind::Set);
    }

    if result.len() != 1 || !options.compact_arrays || active_property == Some("@graph") || active_property == Some("@set") || list_or_set {
        return Ok(jstrict::Value::Array(result.into_iter().collect()));
    }

    // `result.len() == 1` after the early return above, so the fallback
    // is unreachable.
    Ok(result.into_iter().next().unwrap_or(jstrict::Value::Null))
}

impl<T: CompactFragment<I, B>, I, B> CompactFragment<I, B> for IndexSet<T> {
    async fn compact_fragment_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        compact_collection_with(vocabulary, self.iter(), active_context, type_scoped_context, active_property, loader, options).await
    }
}

impl<T: CompactFragment<I, B>, I, B> CompactFragment<I, B> for Vec<T> {
    async fn compact_fragment_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        compact_collection_with(vocabulary, self.iter(), active_context, type_scoped_context, active_property, loader, options).await
    }
}

impl<T: CompactFragment<I, B>, I, B> CompactFragment<I, B> for [T] {
    async fn compact_fragment_full<'a, N, L>(
        &'a self,
        vocabulary: &'a mut N,
        active_context: &'a Context<I, B>,
        type_scoped_context: &'a Context<I, B>,
        active_property: Option<&'a str>,
        loader: &'a L,
        options: Options,
    ) -> CompactFragmentResult<L::Error>
    where
        N: VocabularyMut<Iri = I, BlankId = B>,
        I: Clone + Hash + Eq,
        B: Clone + Hash + Eq,
        L: Loader,
    {
        compact_collection_with(vocabulary, self.iter(), active_context, type_scoped_context, active_property, loader, options).await
    }
}
