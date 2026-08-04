use std::{hash::Hash, sync::Arc};

use super::{DefinedTerms, Environment, Merged};
use crate::{Error, Options, ProcessingStack, Warning, WarningHandler};
use contextual::WithContext;
use iri_rs::{Iri, IriRef};
use jsonld_core::{Context, Id, Loader, ProcessingMode, Term, warning};
use jsonld_syntax::{self as syntax, ExpandableRef, Nullable, context::definition::Key};
use rdfx::{
    BlankId,
    vocabulary::{BlankIdVocabulary, IriVocabulary, Vocabulary, VocabularyMut},
};
use syntax::{CompactIri, context::definition::KeyOrKeywordRef, is_keyword_like};

/// Warning reporting that a value expanded to something that is not a valid IRI.
///
/// Carries the value as it was written. A dedicated type rather than a
/// [`Warning`] variant so that the algorithm can raise it against any warning
/// vocabulary that knows how to build itself `From<MalformedIri>` —
/// `jsonld-expansion` has its own warning enum and reuses this.
pub struct MalformedIri(pub String);

impl From<MalformedIri> for Warning {
    fn from(MalformedIri(s): MalformedIri) -> Self {
        Self::MalformedIri(s)
    }
}

/// Result of [`expand_iri_with`].
///
/// `Ok(None)` means the value was dropped under [`Action::Drop`], as opposed to
/// expanding to [`Term::Null`].
pub type ExpandIriResult<T, B, E> = Result<Option<Arc<Term<T, B>>>, Error<E>>;

/// Runs the [IRI expansion algorithm][1] on `value`, defining terms of
/// `local_context` on demand.
///
/// Used while a context is still being processed, which is why it can recurse
/// into [`define`][super::define]: expanding a term may require first defining
/// that term, or the prefix of a compact IRI, from the context currently being
/// built. `defined` guards that recursion against cycles.
///
/// `document_relative` allows a value that is otherwise unresolvable to be
/// resolved against the active context's base IRI. `vocab` enables expansion
/// through the vocabulary mapping and selects what to do when that is the only
/// thing that would make the value expand; `None` disables it entirely.
///
/// A value that resolves to nothing usable is returned as an invalid identifier
/// and reported through the warning handler, rather than failing.
///
/// [1]: https://www.w3.org/TR/json-ld11-api/#iri-expansion
pub async fn expand_iri_with<'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a mut Context<N::Iri, N::BlankId>,
    value: Nullable<ExpandableRef<'a>>,
    document_relative: bool,
    vocab: Option<Action>,
    local_context: &'a Merged<'a>,
    defined: &'a mut DefinedTerms,
    remote_contexts: ProcessingStack<N::Iri>,
    options: Options,
) -> ExpandIriResult<N::Iri, N::BlankId, L::Error>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + PartialEq,
    L: Loader,
    W: WarningHandler<N>,
{
    match value {
        Nullable::Null => Ok(Some(Arc::new(Term::Null))),
        Nullable::Some(ExpandableRef::Keyword(k)) => Ok(Some(Arc::new(Term::Keyword(k)))),
        Nullable::Some(ExpandableRef::String(value)) => {
            if is_keyword_like(value) {
                return Ok(Some(Arc::new(Term::Null)));
            }

            // If `local_context` is not null, it contains an entry with a key that equals value, and the
            // value of the entry for value in defined is not true, invoke the Create Term Definition
            // algorithm, passing active context, local context, value as term, and defined. This will
            // ensure that a term definition is created for value in active context during Context
            // Processing.
            Box::pin(super::define(
                Environment {
                    vocabulary: env.vocabulary,
                    loader: env.loader,
                    warnings: env.warnings,
                },
                active_context,
                local_context,
                value.into(),
                defined,
                remote_contexts.clone(),
                None,
                false,
                options.with_no_override(),
            ))
            .await?;

            if let Some(term_definition) = active_context.get(value) {
                // If active context has a term definition for value, and the associated IRI mapping
                // is a keyword, return that keyword.
                if let Some(arc) = term_definition.value_arc()
                    && arc.is_keyword()
                {
                    return Ok(Some(Arc::clone(arc)));
                }

                // If vocab is true and the active context has a term definition for value, return the
                // associated IRI mapping.
                if vocab.is_some() {
                    return match term_definition.value_arc() {
                        Some(arc) => Ok(Some(Arc::clone(arc))),
                        None => Ok(Some(Arc::new(Term::Null))),
                    };
                }
            }

            if value.find(':').map(|i| i > 0).unwrap_or(false) {
                if let Ok(blank_id) = BlankId::new(value) {
                    return Ok(Some(Arc::new(Term::Id(Id::blank(env.vocabulary.insert_blank_id(blank_id))))));
                }

                if value == "_:" {
                    return Ok(Some(Arc::new(Term::Id(Id::Invalid("_:".to_string())))));
                }

                if let Ok(compact_iri) = CompactIri::new(value) {
                    // If local context is not null, it contains a `prefix` entry, and the value of the
                    // prefix entry in defined is not true, invoke the Create Term Definition
                    // algorithm, passing active context, local context, prefix as term, and defined.
                    // This will ensure that a term definition is created for prefix in active context
                    // during Context Processing.
                    Box::pin(super::define(
                        Environment {
                            vocabulary: env.vocabulary,
                            loader: env.loader,
                            warnings: env.warnings,
                        },
                        active_context,
                        local_context,
                        KeyOrKeywordRef::Key(compact_iri.prefix().into()),
                        defined,
                        remote_contexts,
                        None,
                        false,
                        options.with_no_override(),
                    ))
                    .await?;

                    // If active context contains a term definition for prefix having a non-null IRI
                    // mapping and the prefix flag of the term definition is true, return the result
                    // of concatenating the IRI mapping associated with prefix and suffix.
                    // The `prefix` flag is a JSON-LD 1.1 addition: in 1.0 any term
                    // definition with an IRI mapping expands a compact IRI
                    // (`flatten#t0014`). Compaction keeps the 1.1 rule, which is why
                    // this is relaxed here and not on the flag itself (`compact#tp001`).
                    let prefix_key = Key::from(compact_iri.prefix());
                    if let Some(term_definition) = active_context.get_normal(&prefix_key)
                        && (term_definition.prefix || active_context.processing_mode() == ProcessingMode::JsonLd1_0)
                        && let Some(mapping) = term_definition.value()
                    {
                        let mut result = mapping.with(&*env.vocabulary).as_str().to_string();
                        result.push_str(compact_iri.suffix());

                        return Ok(Some(Arc::new(Term::Id(Id::from_string_in(env.vocabulary, result)))));
                    }
                }

                if let Ok(iri) = Iri::parse(value) {
                    return Ok(Some(Arc::new(Term::Id(Id::iri(env.vocabulary.insert(iri))))));
                }
            }

            // If vocab is true, and active context has a vocabulary mapping, return the result of
            // concatenating the vocabulary mapping with value.
            if let Some(action) = vocab {
                match active_context.vocabulary() {
                    Some(Term::Id(mapping)) => {
                        return match action {
                            Action::Keep => {
                                let mut result = mapping.with(&*env.vocabulary).as_str().to_string();
                                result.push_str(value);

                                Ok(Some(Arc::new(Term::Id(Id::from_string_in(env.vocabulary, result)))))
                            }
                            Action::Drop => Ok(None),
                            Action::Reject => Err(Error::ForbiddenVocab),
                        };
                    }
                    Some(_) => return Ok(Some(Arc::new(invalid_iri(&mut env, value.to_string())))),
                    None => (),
                }
            }

            // Otherwise, if document relative is true set value to the result of resolving value
            // against the base IRI from active context. Only the basic algorithm in section 5.2 of
            // [RFC3986] is used; neither Syntax-Based Normalization nor Scheme-Based Normalization
            // are performed. Characters additionally allowed in IRI references are treated in the
            // same way that unreserved characters are treated in URI references, per section 6.5 of
            // [RFC3987].
            if document_relative
                && let Ok(iri_ref) = IriRef::parse(value)
                && let Some(iri) = super::resolve_iri(env.vocabulary, iri_ref, active_context.base_iri())
            {
                return Ok(Some(Arc::new(Term::from(iri))));
            }

            // Return value as is.
            Ok(Some(Arc::new(invalid_iri(&mut env, value.to_string()))))
        }
    }
}

/// Reports `value` as a malformed IRI and returns it as an invalid identifier.
///
/// Expansion never fails on an unresolvable value: it keeps it verbatim so the
/// entry survives, and leaves it to the caller's policy to reject it later.
fn invalid_iri<N, L, W: jsonld_core::warning::Handler<N, Warning>>(env: &mut Environment<N, L, W>, value: String) -> Term<N::Iri, N::BlankId>
where
    N: Vocabulary,
{
    env.warnings.handle(env.vocabulary, MalformedIri(value.clone()).into());
    Term::Id(Id::Invalid(value))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
/// How strictly to treat a term whose expansion is questionable.
///
/// Used by this crate for terms that only expand because the active context
/// defines a vocabulary mapping (see [`Options::vocab`][crate::Options::vocab]),
/// and by `jsonld-expansion` for those as well as for terms that expand to a
/// malformed IRI.
pub enum Action {
    #[default]
    /// Accept the expansion. This is the behaviour the specification requires.
    Keep,
    /// Treat the term as unexpandable, so that whatever entry carries it is
    /// dropped.
    Drop,
    /// Abort processing with an error.
    Reject,
}

impl Action {
    /// Checks whether this action aborts processing rather than accepting or
    /// dropping the term.
    pub fn is_reject(&self) -> bool {
        matches!(self, Self::Reject)
    }
}

#[derive(Debug)]
/// A term could only be expanded through the vocabulary mapping, while
/// [`Options::vocab`][crate::Options::vocab] was set to [`Action::Reject`].
///
/// Reported by the loader-free expansion path, which cannot construct an
/// [`Error`]; callers convert it into [`Error::ForbiddenVocab`].
pub struct RejectVocab;

/// Result of expanding a term into an IRI without a loader.
///
/// `Ok(None)` means the term was dropped under [`Action::Drop`], as opposed to
/// expanding to [`Term::Null`].
pub type IriExpansionResult<N> = Result<Option<Arc<Term<<N as IriVocabulary>::Iri, <N as BlankIdVocabulary>::BlankId>>>, RejectVocab>;

/// Expands `value` into an IRI using only the term definitions `active_context`
/// already has.
///
/// The loader-free counterpart of [`expand_iri_with`]: it never creates a term
/// definition on demand, so it can only be used once the enclosing context has
/// been processed, or for values that cannot introduce a dependency (`@vocab`,
/// `@index`, a relative-IRI term).
///
/// `document_relative` allows a value that is otherwise unresolvable to be
/// resolved against the active context's base IRI. `vocab` enables expansion
/// through the vocabulary mapping and selects what to do when that is the only
/// thing that would make the value expand; `None` disables it entirely.
///
/// Results are memoized per active context for the dominant call shape — a
/// string value with `vocab: Some(Action::Keep)` and `document_relative` off —
/// which is what makes repeated keys across the nodes of a document cheap.
pub fn expand_iri_simple<W, N, L, H>(
    env: &mut Environment<N, L, H>,
    active_context: &Context<N::Iri, N::BlankId>,
    value: Nullable<ExpandableRef>,
    document_relative: bool,
    vocab: Option<Action>,
) -> IriExpansionResult<N>
where
    N: VocabularyMut,
    N::Iri: Clone,
    N::BlankId: Clone,
    W: From<MalformedIri>,
    H: warning::Handler<N, W>,
{
    // Per-context memoization of the dominant key-expansion call shape:
    // `vocab=Some(Keep)`, `document_relative=false`, value is a string. Hits
    // skip term-definition lookup, blank/CompactIRI/Iri::parse validators and
    // the vocab-fallback string concat + IRI re-parse + Arc::new for repeated
    // keys (the common NGSI-LD / schema.org pattern).
    let cache_value: Option<&str> = if !document_relative && matches!(vocab, Some(Action::Keep)) {
        if let Nullable::Some(ExpandableRef::String(s)) = &value {
            Some(*s)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(s) = cache_value
        && let Some(arc) = active_context.term_resolution_cache().lock().get(s).cloned()
    {
        return Ok(Some(arc));
    }

    let result = expand_iri_simple_inner::<W, N, L, H>(env, active_context, value, document_relative, vocab)?;

    if let (Some(s), Some(arc)) = (cache_value, &result) {
        active_context.term_resolution_cache().lock().insert(Box::from(s), Arc::clone(arc));
    }

    Ok(result)
}

/// The body of [`expand_iri_simple`], without the memoization wrapper.
fn expand_iri_simple_inner<W, N, L, H>(
    env: &mut Environment<N, L, H>,
    active_context: &Context<N::Iri, N::BlankId>,
    value: Nullable<ExpandableRef>,
    document_relative: bool,
    vocab: Option<Action>,
) -> IriExpansionResult<N>
where
    N: VocabularyMut,
    N::Iri: Clone,
    N::BlankId: Clone,
    W: From<MalformedIri>,
    H: warning::Handler<N, W>,
{
    match value {
        Nullable::Null => Ok(Some(Arc::new(Term::Null))),
        Nullable::Some(ExpandableRef::Keyword(k)) => Ok(Some(Arc::new(Term::Keyword(k)))),
        Nullable::Some(ExpandableRef::String(value)) => {
            if is_keyword_like(value) {
                return Ok(Some(Arc::new(Term::Null)));
            }

            if let Some(term_definition) = active_context.get(value) {
                // If active context has a term definition for value, and the associated IRI mapping
                // is a keyword, return that keyword.
                if let Some(arc) = term_definition.value_arc()
                    && arc.is_keyword()
                {
                    return Ok(Some(Arc::clone(arc)));
                }

                // If vocab is true and the active context has a term definition for value, return the
                // associated IRI mapping.
                if vocab.is_some() {
                    return match term_definition.value_arc() {
                        Some(arc) => Ok(Some(Arc::clone(arc))),
                        None => Ok(Some(Arc::new(Term::Null))),
                    };
                }
            }

            if value.find(':').map(|i| i > 0).unwrap_or(false) {
                if let Ok(blank_id) = BlankId::new(value) {
                    return Ok(Some(Arc::new(Term::Id(Id::blank(env.vocabulary.insert_blank_id(blank_id))))));
                }

                if value == "_:" {
                    return Ok(Some(Arc::new(Term::Id(Id::Invalid("_:".to_string())))));
                }

                if let Ok(compact_iri) = CompactIri::new(value) {
                    // If active context contains a term definition for prefix having a non-null IRI
                    // mapping and the prefix flag of the term definition is true, return the result
                    // of concatenating the IRI mapping associated with prefix and suffix.
                    // The `prefix` flag is a JSON-LD 1.1 addition: in 1.0 any term
                    // definition with an IRI mapping expands a compact IRI
                    // (`flatten#t0014`). Compaction keeps the 1.1 rule, which is why
                    // this is relaxed here and not on the flag itself (`compact#tp001`).
                    let prefix_key = Key::from(compact_iri.prefix());
                    if let Some(term_definition) = active_context.get_normal(&prefix_key)
                        && (term_definition.prefix || active_context.processing_mode() == ProcessingMode::JsonLd1_0)
                        && let Some(mapping) = term_definition.value()
                    {
                        let mut result = mapping.with(&*env.vocabulary).as_str().to_string();
                        result.push_str(compact_iri.suffix());

                        return Ok(Some(Arc::new(Term::Id(Id::from_string_in(env.vocabulary, result)))));
                    }
                }

                if let Ok(iri) = Iri::parse(value) {
                    return Ok(Some(Arc::new(Term::Id(Id::iri(env.vocabulary.insert(iri))))));
                }
            }

            // If vocab is true, and active context has a vocabulary mapping, return the result of
            // concatenating the vocabulary mapping with value.
            if let Some(action) = vocab {
                match active_context.vocabulary() {
                    Some(Term::Id(mapping)) => {
                        return match action {
                            Action::Keep => {
                                let mut result = mapping.with(&*env.vocabulary).as_str().to_string();
                                result.push_str(value);

                                Ok(Some(Arc::new(Term::Id(Id::from_string_in(env.vocabulary, result)))))
                            }
                            Action::Drop => Ok(None),
                            Action::Reject => Err(RejectVocab),
                        };
                    }
                    Some(_) => return Ok(Some(Arc::new(invalid_iri_simple(env, value.to_string())))),
                    None => (),
                }
            }

            // Otherwise, if document relative is true set value to the result of resolving value
            // against the base IRI from active context. Only the basic algorithm in section 5.2 of
            // [RFC3986] is used; neither Syntax-Based Normalization nor Scheme-Based Normalization
            // are performed. Characters additionally allowed in IRI references are treated in the
            // same way that unreserved characters are treated in URI references, per section 6.5 of
            // [RFC3987].
            if document_relative
                && let Ok(iri_ref) = IriRef::parse(value)
                && let Some(iri) = super::resolve_iri(env.vocabulary, iri_ref, active_context.base_iri())
            {
                return Ok(Some(Arc::new(Term::from(iri))));
            }

            // Return value as is.
            Ok(Some(Arc::new(invalid_iri_simple(env, value.to_string()))))
        }
    }
}

/// Reports `value` as a malformed IRI and returns it as an invalid identifier.
///
/// Same as [`invalid_iri`], for the loader-free path's more general warning
/// handler.
fn invalid_iri_simple<W, N, L, H>(env: &mut Environment<N, L, H>, value: String) -> Term<N::Iri, N::BlankId>
where
    N: Vocabulary,
    W: From<MalformedIri>,
    H: warning::Handler<N, W>,
{
    env.warnings.handle(env.vocabulary, MalformedIri(value.clone()).into());
    Term::Id(Id::Invalid(value))
}
