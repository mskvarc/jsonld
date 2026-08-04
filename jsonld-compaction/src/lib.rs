//! This library implements the [JSON-LD compaction algorithm](https://www.w3.org/TR/json-ld-api/#compaction-algorithms)
//! for the [`json-ld` crate](https://crates.io/crates/json-ld).
//!
//! # Usage
//!
//! The compaction algorithm is provided by the [`Compact`] trait.
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
    /// IRI confused with prefix.
    IriConfusedWithPrefix,

    #[error("Invalid `@nest` value")]
    /// Invalid `@nest` value.
    InvalidNestValue,

    #[error("Colliding compacted entry")]
    /// The compaction target of a map-based container or `@nest` entry
    /// already holds a non-map value: two terms compacted to the same key
    /// with incompatible shapes. The JSON-LD specification does not define
    /// an output for this state.
    CollidingEntry,

    #[error("Context processing failed: {0}")]
    /// Context processing failed: the given value.
    ContextProcessing(jsonld_context_processing::Error<E>),
}

impl<E> Error<E> {
    /// Returns the code of this `Error`.
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

    /// Determines if IRIs are compacted relative to the provided base IRI or document location when compacting.
    ///
    /// This crate itself never reads this flag: IRIs are relativized whenever
    /// the active context has a base IRI. The flag is honored by the
    /// higher-level `jsonld` processor, which only seeds the active context's
    /// base IRI when it is set.
    pub compact_to_relative: bool,

    /// If set to `true`, arrays with just one element are replaced with that element during compaction.
    /// If set to `false`, all arrays will remain arrays even if they have just one element.
    pub compact_arrays: bool,

    /// If set to `true`, properties are processed by lexical order.
    /// If `false`, order is not considered in processing.
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
    /// Compacts this fragment, taking every parameter explicitly.
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
    /// Compacts this fragment using the given vocabulary.
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
    /// Compacts this fragment against the active context.
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

/// Type that can be compacted with an index.
pub trait CompactIndexedFragment<I, B> {
    /// Compacts this fragment, keeping the `@index` it was reached through.
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
                // FIXME https://github.com/w3c/json-ld-api/issues/502
                //       Seems that the term definition should be looked up in `type_scoped_context`.
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

/// Default value of `as_array` is false.
///
/// Refactored from a 3-lookup-per-call shape (peek + remove+reinsert + insert)
/// to a single `get_unique_mut` per scalar call. Array values still recurse,
/// but the scalar path — which is the common case — now hits the indexmap
/// once.
fn add_value(map: &mut jstrict::Object, key: &str, value: jstrict::Value, as_array: bool) {
    match value {
        jstrict::Value::Array(values) => {
            // Pre-arrange the entry shape exactly as the original two-pass code did:
            // wrap an existing scalar into a single-element array, or insert an
            // empty array when `as_array` is set and the entry is absent.
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

/// Get the `@value` field of a value object.
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
