//! Synchronous mirror of the context processing algorithm.
//!
//! # Two implementations, one algorithm: keep `mod.rs` in step
//!
//! This module is a second, hand-maintained implementation of the *same*
//! algorithm as the `async` code in `mod.rs`, `define.rs` and `iri.rs`, with
//! `async`, `.await` and `Box::pin` stripped out and recursion made direct. It
//! is used as a fast path whenever [`requires_loader`] proves the input contains
//! no remote `@context` IRI and no `@import`, which is the common case in real
//! payloads (NGSI-LD, schema.org and similar).
//!
//! Which of the two runs is invisible to callers: for any input both must
//! produce the same active context and the same error. **A change to the
//! algorithm here therefore has to be mirrored in the `async` files, and a
//! change there mirrored back here.** Nothing enforces this — the two are
//! separate function bodies with no shared code, so a fix applied to only one
//! silently makes the result depend on whether the input happened to reference a
//! remote context. The mirrored pairs are:
//!
//! | this module | `async` counterpart |
//! |---|---|
//! | [`process_context_sync`] | `process_context` (`mod.rs`) |
//! | [`define_sync`] | `define` (`define.rs`) |
//! | [`expand_iri_with_sync`] | `expand_iri_with` (`iri.rs`) |
//!
//! The commentary quoting the specification's steps is deliberately not
//! duplicated here; it lives on the `async` versions, which are the ones to read
//! when auditing the code against the specification.
//!
//! # Loader tripwire
//!
//! Reaching a code path that the `async` version would handle by awaiting the
//! loader is treated as a bug rather than silently skipped: the functions here
//! return [`Error::LoadingDocumentFailed`] instead. Since
//! [`Process::process_full`][crate::Process::process_full] only dispatches here
//! after [`requires_loader`] has said no loader is needed, that should be
//! unreachable — it exists to catch the pre-scan drifting out of step with the
//! algorithm body.
//!
//! # Recursion depth
//!
//! The `async` version heap-allocates each recursion level through `Box::pin`;
//! this one recurses on the native stack, so it is bounded by
//! [`MAX_SYNC_DEPTH`]. A crafted deeply nested `@context` fails with
//! [`Error::ContextOverflow`] rather than overflowing the stack.

use super::{DefinedTerms, Environment, Merged, expand_iri_simple, is_legacy_vocab, resolve_iri};
use crate::{
    Error,
    Options,
    ProcessingStack,
    Warning,
    WarningHandler,
    algorithm::iri::{Action, MalformedIri},
};
use contextual::WithContext;
use iri_rs::{Iri, IriRef};
use jsonld_core::{
    Container,
    Context,
    Id,
    Loader,
    ProcessingMode,
    Term,
    Type,
    ValidId,
    context::{NormalTermDefinition, TypeTermDefinition},
};
use jsonld_syntax::{
    self as syntax,
    CompactIri,
    ContainerKind,
    ExpandableRef,
    Keyword,
    LenientLangTag,
    Nullable,
    context::{
        definition::{EntryValueRef, Key, KeyOrKeyword, KeyOrKeywordRef},
        term_definition::{self, IdRef},
    },
};
use rdfx::{BlankId, vocabulary::VocabularyMut};
use std::{hash::Hash, sync::Arc};

/// Maximum recursion depth of the synchronous context-processing fast path.
///
/// The async algorithm heap-allocates every recursion level through
/// `Box::pin`; the sync mirror recurses on the native stack, so a crafted
/// deeply-nested inline `@context` could otherwise overflow it (an abort,
/// not UB). Exceeding the limit reports [`Error::ContextOverflow`]. Genuine
/// contexts stay far below this bound: depth grows with the nesting of
/// scoped contexts and chained term/prefix definitions, not with context
/// size. The value is chosen so that even unoptimized builds (with their
/// much larger stack frames) stay within a 2 MiB thread stack.
const MAX_SYNC_DEPTH: usize = 128;

type ExpandIriResult<N, L> =
    Result<Option<Arc<Term<<N as rdfx::vocabulary::IriVocabulary>::Iri, <N as rdfx::vocabulary::BlankIdVocabulary>::BlankId>>>, Error<<L as Loader>::Error>>;
type ProcessContextResult<'l, N, L> =
    Result<crate::Processed<'l, <N as rdfx::vocabulary::IriVocabulary>::Iri, <N as rdfx::vocabulary::BlankIdVocabulary>::BlankId>, Error<<L as Loader>::Error>>;

/// Checks whether processing `ctx` would have to fetch a document.
///
/// Returns `true` if `ctx`, or any context nested in one of its term
/// definitions' `@context` entries, references a remote `@context` by IRI or
/// carries an `@import`. When it returns `false` the whole algorithm can run
/// without the loader, and therefore synchronously.
pub fn requires_loader(ctx: &syntax::context::Context) -> bool {
    for entry in ctx {
        if entry_requires_loader(entry) {
            return true;
        }
    }
    false
}

/// Checks one entry of a context for anything the loader would have to fetch.
///
/// A bare IRI reference always needs the loader. A context definition needs it
/// for `@import`, or if any of its term definitions carries a scoped `@context`
/// that in turn needs it.
fn entry_requires_loader(entry: &syntax::ContextEntry) -> bool {
    match entry {
        syntax::ContextEntry::Null => false,
        syntax::ContextEntry::IriRef(_) => true,
        syntax::ContextEntry::Definition(def) => {
            if def.import.is_some() {
                return true;
            }
            for (_, binding) in def.bindings.iter() {
                let term_def = match binding {
                    Nullable::Null => continue,
                    Nullable::Some(d) => d,
                };
                if let term_definition::TermDefinition::Expanded(e) = term_def
                    && let Some(nested) = e.context.as_deref()
                    && requires_loader(nested)
                {
                    return true;
                }
            }
            false
        }
    }
}

// The three helpers below are byte-for-byte copies of the ones in `define.rs`.
// They are duplicated rather than shared because they are private to that
// module; keep the copies identical.

/// Checks whether `c` is one of RFC 3986's generic delimiters.
fn is_gen_delim(c: char) -> bool {
    matches!(c, ':' | '/' | '?' | '#' | '[' | ']' | '@')
}

/// Checks whether `t` is a blank node identifier, or an IRI whose last character
/// is a generic delimiter.
fn is_gen_delim_or_blank<T, B>(vocabulary: &impl VocabularyMut<Iri = T, BlankId = B>, t: &Term<T, B>) -> bool {
    match t {
        Term::Id(Id::Valid(ValidId::Blank(_))) => true,
        Term::Id(Id::Valid(ValidId::Iri(id))) => match vocabulary.iri(id).and_then(|i| i.as_str().chars().last()) {
            Some(c) => is_gen_delim(c),
            None => false,
        },
        _ => false,
    }
}

/// Checks whether `c` occurs in `id`, but as neither its first nor its last
/// character.
fn contains_between_boundaries(id: &str, c: char) -> bool {
    if let Some(i) = id.find(c) {
        // SAFETY: `find` matched, so `rfind` must also match.
        let j = unsafe { id.rfind(c).unwrap_unchecked() };
        i > 0 && j < id.len() - 1
    } else {
        false
    }
}

/// Loader-free mirror of `expand_iri_with` in `iri.rs`. Changes to either must be
/// applied to both — see the module documentation.
///
/// `depth` is the current recursion depth, checked against [`MAX_SYNC_DEPTH`] by
/// [`define_sync`], which this function recurses through.
pub fn expand_iri_with_sync<'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a mut Context<N::Iri, N::BlankId>,
    value: Nullable<ExpandableRef<'a>>,
    document_relative: bool,
    vocab: Option<Action>,
    local_context: &'a Merged<'a>,
    defined: &'a mut DefinedTerms,
    remote_contexts: ProcessingStack<N::Iri>,
    options: Options,
    depth: usize,
) -> ExpandIriResult<N, L>
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
            if syntax::is_keyword_like(value) {
                return Ok(Some(Arc::new(Term::Null)));
            }

            define_sync(
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
                depth + 1,
            )?;

            if let Some(term_definition) = active_context.get(value) {
                if let Some(arc) = term_definition.value_arc()
                    && arc.is_keyword()
                {
                    return Ok(Some(Arc::clone(arc)));
                }

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
                    define_sync(
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
                        depth + 1,
                    )?;

                    // The `prefix` flag is JSON-LD 1.1 only; see `iri.rs`.
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

            if document_relative
                && let Ok(iri_ref) = IriRef::parse(value)
                && let Some(iri) = resolve_iri(env.vocabulary, iri_ref, active_context.base_iri())
            {
                return Ok(Some(Arc::new(Term::from(iri))));
            }

            Ok(Some(Arc::new(invalid_iri(&mut env, value.to_string()))))
        }
    }
}

/// Reports `value` as a malformed IRI and returns it as an invalid identifier.
///
/// Mirrors the identically named helper in `iri.rs`.
fn invalid_iri<N, L, W: jsonld_core::warning::Handler<N, Warning>>(env: &mut Environment<N, L, W>, value: String) -> Term<N::Iri, N::BlankId>
where
    N: rdfx::vocabulary::Vocabulary,
{
    env.warnings.handle(env.vocabulary, MalformedIri(value.clone()).into());
    Term::Id(Id::Invalid(value))
}

/// Loader-free mirror of `define` in `define.rs`. Changes to either must be
/// applied to both — see the module documentation.
///
/// `depth` is the current recursion depth; exceeding [`MAX_SYNC_DEPTH`] returns
/// [`Error::ContextOverflow`] rather than risking a stack overflow.
pub fn define_sync<'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a mut Context<N::Iri, N::BlankId>,
    local_context: &'a Merged<'a>,
    term: KeyOrKeywordRef<'a>,
    defined: &'a mut DefinedTerms,
    remote_contexts: ProcessingStack<N::Iri>,
    base_url: Option<N::Iri>,
    protected: bool,
    options: Options,
    depth: usize,
) -> Result<(), Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + PartialEq,
    L: Loader,
    W: WarningHandler<N>,
{
    if depth >= MAX_SYNC_DEPTH {
        return Err(Error::ContextOverflow);
    }

    let term = term.to_owned();
    if defined.begin(&term)? {
        if term.is_empty() {
            return Err(Error::InvalidTermDefinition);
        }

        if let Some(value) = local_context.get(&term) {
            match value {
                EntryValueRef::Type(d) => {
                    if options.processing_mode == ProcessingMode::JsonLd1_0 {
                        return Err(Error::KeywordRedefinition);
                    }

                    let previous_definition = active_context.set_type(None);

                    let mut definition = TypeTermDefinition {
                        container: d.container,
                        ..Default::default()
                    };

                    if let Some(protected) = d.protected {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        definition.protected = protected
                    }

                    if !options.override_protected
                        && let Some(previous_definition) = previous_definition
                        && previous_definition.protected
                    {
                        if definition.modulo_protected_field() != previous_definition.modulo_protected_field() {
                            return Err(Error::ProtectedTermRedefinition);
                        }

                        definition.protected = true;
                    }

                    active_context.set_type(Some(definition));
                }
                EntryValueRef::Definition(d) => {
                    // SAFETY: at this point `term` is a `KeyOrKeyword::Key`,
                    // since the `Type` arm above returns early.
                    let key = unsafe { term.as_key().unwrap_unchecked() };
                    let previous_definition = active_context.set_normal(*key, None);

                    let simple_term = !d.map(|d| d.is_expanded()).unwrap_or(false);
                    let value = term_definition::ExpandedRef::from(d);

                    let mut definition = NormalTermDefinition::<N::Iri, N::BlankId> {
                        protected,
                        ..Default::default()
                    };

                    if let Some(protected) = value.protected {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        definition.protected = protected;
                    }

                    if let Some(type_) = value.type_ {
                        let typ = expand_iri_with_sync(
                            Environment {
                                vocabulary: env.vocabulary,
                                loader: env.loader,
                                warnings: env.warnings,
                            },
                            active_context,
                            type_.cast(),
                            false,
                            Some(options.vocab),
                            local_context,
                            defined,
                            remote_contexts.clone(),
                            options,
                            depth + 1,
                        )?;

                        if let Some(typ) = typ {
                            let typ = Arc::try_unwrap(typ).unwrap_or_else(|a| (*a).clone());
                            if options.processing_mode == ProcessingMode::JsonLd1_0
                                && (typ == Term::Keyword(Keyword::Json) || typ == Term::Keyword(Keyword::None))
                            {
                                return Err(Error::InvalidTypeMapping);
                            }

                            if let Ok(typ) = typ.try_into() {
                                definition.typ = Some(typ);
                            } else {
                                return Err(Error::InvalidTypeMapping);
                            }
                        }
                    }

                    if let Some(reverse_value) = value.reverse {
                        if value.id.is_some() || value.nest.is_some() {
                            return Err(Error::InvalidReverseProperty);
                        }

                        if reverse_value.is_keyword_like() {
                            env.warnings.handle(env.vocabulary, Warning::KeywordLikeValue(reverse_value.to_string()));
                            return Ok(());
                        }

                        match expand_iri_with_sync(
                            Environment {
                                vocabulary: env.vocabulary,
                                loader: env.loader,
                                warnings: env.warnings,
                            },
                            active_context,
                            Nullable::Some(reverse_value.as_str().into()),
                            false,
                            Some(options.vocab),
                            local_context,
                            defined,
                            remote_contexts.clone(),
                            options,
                            depth + 1,
                        )? {
                            Some(arc) if matches!(arc.as_ref(), Term::Id(m) if m.is_valid()) => definition.value = Some(arc),
                            _ => return Err(Error::InvalidIriMapping),
                        }

                        if let Some(container_value) = value.container {
                            match container_value {
                                Nullable::Null => (),
                                Nullable::Some(container_value) => {
                                    let container_value = Container::from_syntax(Nullable::Some(container_value)).map_err(|_| Error::InvalidReverseProperty)?;

                                    if matches!(container_value, Container::Set | Container::Index) {
                                        definition.container = container_value
                                    } else {
                                        return Err(Error::InvalidReverseProperty);
                                    }
                                }
                            };
                        }

                        definition.reverse_property = true;
                    }

                    // Reverse properties skip the `@id` branch; see `define.rs`.
                    if !definition.reverse_property {
                        match value.id {
                            Some(id_value) if id_value.cast::<KeyOrKeywordRef>() != Nullable::Some(key.into()) => match id_value {
                                Nullable::Null => (),
                                Nullable::Some(id_value) => {
                                    if id_value.is_keyword_like() && !id_value.is_keyword() {
                                        debug_assert!(Keyword::try_from(id_value.as_str()).is_err());
                                        env.warnings.handle(env.vocabulary, Warning::KeywordLikeValue(id_value.to_string()));
                                        return Ok(());
                                    }

                                    definition.value = match expand_iri_with_sync(
                                        Environment {
                                            vocabulary: env.vocabulary,
                                            loader: env.loader,
                                            warnings: env.warnings,
                                        },
                                        active_context,
                                        Nullable::Some(id_value.into()),
                                        false,
                                        Some(options.vocab),
                                        local_context,
                                        defined,
                                        remote_contexts.clone(),
                                        options,
                                        depth + 1,
                                    )? {
                                        Some(arc) if arc.as_ref() == &Term::Keyword(Keyword::Context) => {
                                            return Err(Error::InvalidKeywordAlias);
                                        }
                                        Some(arc) if matches!(arc.as_ref(), Term::Id(p) if !p.is_valid()) => {
                                            return Err(Error::InvalidIriMapping);
                                        }
                                        value => value,
                                    };

                                    // The round-trip check below was introduced in JSON-LD 1.1; see the
                                    // matching comment in `define.rs`.
                                    if options.processing_mode != ProcessingMode::JsonLd1_0
                                        && (contains_between_boundaries(key.as_str(), ':') || key.as_str().contains('/'))
                                    {
                                        defined.end(&term);

                                        let expanded_term = expand_iri_with_sync(
                                            Environment {
                                                vocabulary: env.vocabulary,
                                                loader: env.loader,
                                                warnings: env.warnings,
                                            },
                                            active_context,
                                            Nullable::Some((&term).into()),
                                            false,
                                            Some(options.vocab),
                                            local_context,
                                            defined,
                                            remote_contexts.clone(),
                                            options,
                                            depth + 1,
                                        )?;
                                        if definition.value.as_deref() != expanded_term.as_deref() {
                                            return Err(Error::InvalidIriMapping);
                                        }
                                    }

                                    if !key.as_str().contains(':')
                                        && !key.as_str().contains('/')
                                        && simple_term
                                        && definition.value.as_ref().map(|v| is_gen_delim_or_blank(env.vocabulary, v)).unwrap_or(false)
                                    {
                                        definition.prefix = true;
                                    }
                                }
                            },
                            Some(Nullable::Some(IdRef::Keyword(Keyword::Type))) => definition.value = Some(Arc::new(Term::Keyword(Keyword::Type))),
                            _ => {
                                if let KeyOrKeyword::Key(term) = &term {
                                    if let Ok(compact_iri) = CompactIri::new(term.as_str()) {
                                        define_sync(
                                            Environment {
                                                vocabulary: env.vocabulary,
                                                loader: env.loader,
                                                warnings: env.warnings,
                                            },
                                            active_context,
                                            local_context,
                                            KeyOrKeywordRef::Key(compact_iri.prefix().into()),
                                            defined,
                                            remote_contexts.clone(),
                                            None,
                                            false,
                                            options.with_no_override(),
                                            depth + 1,
                                        )?;

                                        if let Some(prefix_definition) = active_context.get(compact_iri.prefix()) {
                                            let mut result = String::new();

                                            if let Some(prefix_key) = prefix_definition.value()
                                                && let Some(prefix_iri) = prefix_key.as_iri()
                                                && let Some(iri) = env.vocabulary.iri(prefix_iri)
                                            {
                                                result = iri.to_string()
                                            }

                                            result.push_str(compact_iri.suffix());

                                            if let Ok(iri) = Iri::parse(result.as_str()) {
                                                definition.value = Some(Arc::new(Term::Id(Id::iri(env.vocabulary.insert(iri)))))
                                            } else {
                                                return Err(Error::InvalidIriMapping);
                                            }
                                        }
                                    }

                                    if definition.value.is_none() {
                                        if let Ok(blank_id) = BlankId::new(term.as_str()) {
                                            definition.value = Some(Arc::new(Term::Id(Id::blank(env.vocabulary.insert_blank_id(blank_id)))))
                                        } else if let Ok(iri_ref) = IriRef::parse(term.as_str()) {
                                            match Iri::try_from(iri_ref) {
                                                Ok(iri) => definition.value = Some(Arc::new(Term::Id(Id::iri(env.vocabulary.insert(iri))))),
                                                Err(_) => {
                                                    if iri_ref.as_str().contains('/') {
                                                        match expand_iri_simple(
                                                            &mut env,
                                                            active_context,
                                                            Nullable::Some(ExpandableRef::String(iri_ref.as_str())),
                                                            false,
                                                            Some(options.vocab),
                                                        )? {
                                                            Some(arc) if matches!(arc.as_ref(), Term::Id(Id::Valid(ValidId::Iri(_)))) => {
                                                                definition.value = Some(arc)
                                                            }
                                                            _ => return Err(Error::InvalidIriMapping),
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        if definition.value.is_none() {
                                            if let Some(context_vocabulary) = active_context.vocabulary() {
                                                if let Some(vocabulary_iri) = context_vocabulary.as_iri() {
                                                    let mut result = env.vocabulary.iri(vocabulary_iri).map(|i| i.to_string()).unwrap_or_default();
                                                    result.push_str(key.as_str());
                                                    if let Ok(iri) = Iri::parse(result.as_str()) {
                                                        definition.value = Some(Arc::new(Term::<N::Iri, N::BlankId>::from(env.vocabulary.insert(iri))))
                                                    } else {
                                                        return Err(Error::InvalidIriMapping);
                                                    }
                                                } else {
                                                    return Err(Error::InvalidIriMapping);
                                                }
                                            } else {
                                                return Err(Error::InvalidIriMapping);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(container_value) = value.container {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            match container_value {
                                Nullable::Null
                                | Nullable::Some(
                                    syntax::Container::Many(_) | syntax::Container::One(ContainerKind::Graph | ContainerKind::Id | ContainerKind::Type),
                                ) => return Err(Error::InvalidContainerMapping),
                                _ => (),
                            }
                        }

                        let container_value = Container::from_syntax(container_value).map_err(|_| Error::InvalidContainerMapping)?;

                        definition.container = container_value;

                        if definition.container.contains(ContainerKind::Type) {
                            if let Some(typ) = &definition.typ {
                                match typ {
                                    Type::Id | Type::Vocab => (),
                                    _ => return Err(Error::InvalidTypeMapping),
                                }
                            } else {
                                definition.typ = Some(Type::Id)
                            }
                        }
                    }

                    if let Some(index_value) = value.index {
                        if !definition.container.contains(ContainerKind::Index) || options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        match expand_iri_simple(
                            &mut env,
                            active_context,
                            Nullable::Some(index_value.as_str().into()),
                            false,
                            Some(options.vocab),
                        )?
                        .as_deref()
                        {
                            Some(Term::Id(Id::Valid(ValidId::Iri(_)))) => (),
                            _ => return Err(Error::InvalidTermDefinition),
                        }

                        definition.index = Some(index_value.to_owned())
                    }

                    if let Some(context) = value.context {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        process_context_sync(
                            env,
                            active_context,
                            context,
                            remote_contexts.clone(),
                            base_url.clone(),
                            options.with_override(),
                            depth + 1,
                        )
                        .map_err(|e| match e {
                            // A resource-limit abort is not a context
                            // error: let it surface instead of masking it.
                            Error::ContextOverflow => Error::ContextOverflow,
                            _ => Error::InvalidScopedContext,
                        })?;

                        definition.context = Some(Box::new(context.clone()));
                        definition.base_url = base_url;
                    }

                    if value.type_.is_none() {
                        if let Some(language_value) = value.language {
                            definition.language = Some(language_value.map(LenientLangTag::to_owned));
                        }

                        if let Some(direction_value) = value.direction {
                            definition.direction = Some(direction_value);
                        }
                    }

                    if let Some(nest_value) = value.nest {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        definition.nest = Some(nest_value.clone());
                    }

                    if let Some(prefix_value) = value.prefix {
                        if key.as_str().contains(':') || key.as_str().contains('/') || options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        definition.prefix = prefix_value;

                        if definition.prefix && definition.value.as_ref().map(|v| v.is_keyword()).unwrap_or(false) {
                            return Err(Error::InvalidTermDefinition);
                        }
                    }

                    if value.propagate.is_some() {
                        return Err(Error::InvalidTermDefinition);
                    }

                    if !options.override_protected
                        && let Some(previous_definition) = previous_definition
                        && previous_definition.protected
                    {
                        if definition.modulo_protected_field() != previous_definition.modulo_protected_field() {
                            return Err(Error::ProtectedTermRedefinition);
                        }

                        definition.protected = true;
                    }

                    active_context.set_normal(key.to_owned(), Some(definition));
                }
                _ => {
                    return Err(Error::KeywordRedefinition);
                }
            }
        }

        defined.end(&term);
    }

    Ok(())
}

/// Loader-free mirror of `process_context` in `mod.rs`. Changes to either must be
/// applied to both — see the module documentation.
///
/// Call only after [`requires_loader`] has returned `false` for `local_context`:
/// meeting a remote `@context` IRI or an `@import` here returns
/// [`Error::LoadingDocumentFailed`] instead of loading it.
pub(crate) fn process_context_sync<'l: 'a, 'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a Context<N::Iri, N::BlankId>,
    local_context: &'l syntax::context::Context,
    remote_contexts: ProcessingStack<N::Iri>,
    base_url: Option<N::Iri>,
    mut options: Options,
    depth: usize,
) -> ProcessContextResult<'l, N, L>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + PartialEq,
    L: Loader,
    W: WarningHandler<N>,
{
    let mut result = active_context.clone();
    result.set_processing_mode(options.processing_mode);

    if let syntax::context::Context::One(syntax::ContextEntry::Definition(def)) = local_context
        && let Some(propagate) = def.propagate
    {
        if options.processing_mode == ProcessingMode::JsonLd1_0 {
            return Err(Error::InvalidContextEntry);
        }

        options.propagate = propagate
    }

    if !options.propagate && result.previous_context().is_none() {
        result.set_previous_context(active_context.clone());
    }

    for context in local_context {
        match context {
            syntax::ContextEntry::Null => {
                if !options.override_protected && result.has_protected_items() {
                    return Err(Error::InvalidContextNullification);
                } else {
                    let previous_result = result;

                    result = Context::new(active_context.original_base_url().cloned());
                    result.set_processing_mode(options.processing_mode);

                    if !options.propagate {
                        result.set_previous_context(previous_result);
                    }
                }
            }

            // `requires_loader` promised there would be no remote context
            // reference. Reaching one means the pre-scan and this function have
            // drifted apart: fail loudly instead of silently skipping the
            // context and returning a wrong active context.
            syntax::ContextEntry::IriRef(_) => {
                return Err(Error::LoadingDocumentFailed);
            }

            syntax::ContextEntry::Definition(context) => {
                if context.version.is_some() && options.processing_mode == ProcessingMode::JsonLd1_0 {
                    return Err(Error::ProcessingModeConflict);
                }

                // Same tripwire as above: `requires_loader` promised no
                // `@import`, which would need the loader to dereference.
                if context.import.is_some() {
                    return Err(Error::LoadingDocumentFailed);
                }

                let context = Merged::new(context, None);

                if remote_contexts.is_empty()
                    && let Some(value) = context.base()
                {
                    match value {
                        syntax::Nullable::Null => {
                            result.set_base_iri(None);
                        }
                        syntax::Nullable::Some(iri_ref) => match Iri::try_from(iri_ref.as_ref()) {
                            Ok(iri) => result.set_base_iri(Some(env.vocabulary.insert(iri))),
                            Err(_) => {
                                let resolved = resolve_iri(env.vocabulary, iri_ref.as_ref(), result.base_iri()).ok_or(Error::InvalidBaseIri)?;
                                result.set_base_iri(Some(resolved))
                            }
                        },
                    }
                }

                if let Some(value) = context.vocab() {
                    match value {
                        syntax::Nullable::Null => {
                            result.set_vocabulary(None);
                        }
                        // Document-relative `@vocab` is a JSON-LD 1.1 addition; see `mod.rs`.
                        syntax::Nullable::Some(value) => {
                            if options.processing_mode == ProcessingMode::JsonLd1_0 && !is_legacy_vocab(value) {
                                return Err(Error::InvalidVocabMapping);
                            }

                            match expand_iri_simple(
                                &mut env,
                                &result,
                                Nullable::Some(value.into()),
                                options.processing_mode != ProcessingMode::JsonLd1_0,
                                Some(options.vocab),
                            )? {
                                Some(arc) if matches!(arc.as_ref(), Term::Id(_)) => {
                                    let term = Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone());
                                    result.set_vocabulary(Some(term));
                                }
                                _ => return Err(Error::InvalidVocabMapping),
                            }
                        }
                    }
                }

                if let Some(value) = context.language() {
                    match value {
                        Nullable::Null => {
                            result.set_default_language(None);
                        }
                        Nullable::Some(tag) => {
                            result.set_default_language(Some(tag.to_owned()));
                        }
                    }
                }

                if let Some(value) = context.direction() {
                    if options.processing_mode == ProcessingMode::JsonLd1_0 {
                        return Err(Error::InvalidContextEntry);
                    }

                    match value {
                        Nullable::Null => {
                            result.set_default_base_direction(None);
                        }
                        Nullable::Some(dir) => {
                            result.set_default_base_direction(Some(dir));
                        }
                    }
                }

                let mut defined = DefinedTerms::new();
                let protected = context.protected().unwrap_or(false);

                if context.type_().is_some() {
                    define_sync(
                        Environment {
                            vocabulary: env.vocabulary,
                            loader: env.loader,
                            warnings: env.warnings,
                        },
                        &mut result,
                        &context,
                        KeyOrKeywordRef::Keyword(syntax::Keyword::Type),
                        &mut defined,
                        remote_contexts.clone(),
                        base_url.clone(),
                        protected,
                        options,
                        depth + 1,
                    )?
                }

                for (key, _binding) in context.bindings() {
                    define_sync(
                        Environment {
                            vocabulary: env.vocabulary,
                            loader: env.loader,
                            warnings: env.warnings,
                        },
                        &mut result,
                        &context,
                        key.into(),
                        &mut defined,
                        remote_contexts.clone(),
                        base_url.clone(),
                        protected,
                        options,
                        depth + 1,
                    )?
                }
            }
        }
    }

    Ok(crate::Processed::new(local_context, result))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use jsonld_core::NoLoader;
    use jsonld_syntax::{Parse, TryFromJson};

    /// Builds `levels` nestings of `{"a": {"@id": ..., "@context": ...}}`.
    fn nested_context(levels: usize) -> syntax::context::Context {
        let mut json = String::new();
        for _ in 0..levels {
            json.push_str("{\"a\":{\"@id\":\"http://example.com/a\",\"@context\":");
        }
        json.push_str("{}");
        for _ in 0..levels {
            json.push_str("}}");
        }

        let (value, _) = jsonld_syntax::Value::parse_str(&json).unwrap();
        syntax::context::Context::try_from_json(&value).unwrap()
    }

    fn process(levels: usize) -> Result<(), crate::ErrorCode> {
        let local = nested_context(levels);
        let active: Context<iri_rs::IriBuf, rdfx::BlankIdBuf> = Context::new(None);
        let mut warnings = ();

        process_context_sync(
            Environment {
                vocabulary: rdfx::vocabulary::no_vocabulary_mut(),
                loader: &NoLoader,
                warnings: &mut warnings,
            },
            &active,
            &local,
            ProcessingStack::default(),
            None,
            crate::Options::default(),
            0,
        )
        .map(|_| ())
        .map_err(|e| e.code())
    }

    /// A crafted deeply-nested inline `@context` must fail with a context
    /// overflow instead of exhausting the native stack.
    #[test]
    fn deeply_nested_inline_context_overflows_gracefully() {
        // 100 nesting levels ≈ 200 recursion depth — past MAX_SYNC_DEPTH but
        // below `jsonld_syntax`'s own MAX_CONTEXT_DEPTH so the scaffolding
        // can build the context at all.
        assert_eq!(process(100), Err(crate::ErrorCode::ContextOverflow));
    }

    #[test]
    fn reasonable_nesting_is_unaffected() {
        assert!(process(20).is_ok());
    }
}
