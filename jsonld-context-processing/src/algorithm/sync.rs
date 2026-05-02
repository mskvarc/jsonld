//! Synchronous mirror of the context-processing algorithm.
//!
//! Functionally identical to the `async` versions in `mod.rs`, `define.rs`,
//! and `iri.rs`, with `async`/`.await`/`Box::pin` removed and recursive calls
//! made direct. Used when [`requires_loader`] proves the input has no remote
//! `@context` IRIs and no `@import` — the common case in production payloads
//! (NGSI-LD, schema.org, etc.).
//!
//! Soundness contract: if any code path is reached that the async version
//! would handle via `loader.load_with(...).await`, the sync version returns
//! [`Error::LoadingDocumentFailed`]. The dispatcher in
//! [`Process::process_full`][crate::Process::process_full] only calls these
//! functions after [`requires_loader`] has been verified, so this fallback
//! should never fire in practice — it exists as a defence against pre-scan
//! drift from the algorithm body.

use super::{DefinedTerms, Environment, Merged, expand_iri_simple, resolve_iri};
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
use rdf_rs::{BlankId, vocabulary::VocabularyMut};
use std::{hash::Hash, sync::Arc};

/// Returns `true` if the given context (or any context nested in a term
/// definition's `@context`) references a remote `@context` IRI or contains
/// `@import`.
///
/// When this returns `false`, the entire algorithm can run without the loader
/// — i.e. synchronously.
pub fn requires_loader(ctx: &syntax::context::Context) -> bool {
    for entry in ctx {
        if entry_requires_loader(entry) {
            return true;
        }
    }
    false
}

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
                if let term_definition::TermDefinition::Expanded(e) = term_def {
                    if let Some(nested) = e.context.as_deref() {
                        if requires_loader(nested) {
                            return true;
                        }
                    }
                }
            }
            false
        }
    }
}

fn is_gen_delim(c: char) -> bool {
    matches!(c, ':' | '/' | '?' | '#' | '[' | ']' | '@')
}

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

fn contains_between_boundaries(id: &str, c: char) -> bool {
    if let Some(i) = id.find(c) {
        // SAFETY: `find` matched, so `rfind` must also match.
        let j = unsafe { id.rfind(c).unwrap_unchecked() };
        i > 0 && j < id.len() - 1
    } else {
        false
    }
}

/// Sync mirror of [`super::expand_iri_with`].
#[allow(clippy::too_many_arguments)]
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
) -> Result<Option<Arc<Term<N::Iri, N::BlankId>>>, Error<L::Error>>
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
            )?;

            if let Some(term_definition) = active_context.get(value) {
                if let Some(arc) = term_definition.value_arc() {
                    if arc.is_keyword() {
                        return Ok(Some(Arc::clone(arc)));
                    }
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
                    )?;

                    let prefix_key = Key::from(compact_iri.prefix());
                    if let Some(term_definition) = active_context.get_normal(&prefix_key) {
                        if term_definition.prefix {
                            if let Some(mapping) = term_definition.value() {
                                let mut result = mapping.with(&*env.vocabulary).as_str().to_string();
                                result.push_str(compact_iri.suffix());

                                return Ok(Some(Arc::new(Term::Id(Id::from_string_in(env.vocabulary, result)))));
                            }
                        }
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

            if document_relative {
                if let Ok(iri_ref) = IriRef::parse(value) {
                    if let Some(iri) = resolve_iri(env.vocabulary, iri_ref, active_context.base_iri()) {
                        return Ok(Some(Arc::new(Term::from(iri))));
                    }
                }
            }

            Ok(Some(Arc::new(invalid_iri(&mut env, value.to_string()))))
        }
    }
}

fn invalid_iri<N, L, W: jsonld_core::warning::Handler<N, Warning>>(env: &mut Environment<N, L, W>, value: String) -> Term<N::Iri, N::BlankId>
where
    N: rdf_rs::vocabulary::Vocabulary,
{
    env.warnings.handle(env.vocabulary, MalformedIri(value.clone()).into());
    Term::Id(Id::Invalid(value))
}

/// Sync mirror of [`super::define`].
#[allow(clippy::too_many_arguments)]
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
) -> Result<(), Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + PartialEq,
    L: Loader,
    W: WarningHandler<N>,
{
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

                    if !options.override_protected {
                        if let Some(previous_definition) = previous_definition {
                            if previous_definition.protected {
                                if definition.modulo_protected_field() != previous_definition.modulo_protected_field() {
                                    return Err(Error::ProtectedTermRedefinition);
                                }

                                definition.protected = true;
                            }
                        }
                    }

                    active_context.set_type(Some(definition));
                }
                EntryValueRef::Definition(d) => {
                    // SAFETY: at this point `term` is a `KeyOrKeyword::Key`,
                    // since the `Type` arm above returns early.
                    let key = unsafe { term.as_key().unwrap_unchecked() };
                    let previous_definition = active_context.set_normal(key.clone(), None);

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
                            env,
                            active_context,
                            Nullable::Some(reverse_value.as_str().into()),
                            false,
                            Some(options.vocab),
                            local_context,
                            defined,
                            remote_contexts,
                            options,
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

                        active_context.set_normal(key.to_owned(), Some(definition));
                        defined.end(&term);
                        return Ok(());
                    }

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
                                )? {
                                    Some(arc) if arc.as_ref() == &Term::Keyword(Keyword::Context) => {
                                        return Err(Error::InvalidKeywordAlias);
                                    }
                                    Some(arc) if matches!(arc.as_ref(), Term::Id(p) if !p.is_valid()) => {
                                        return Err(Error::InvalidIriMapping);
                                    }
                                    value => value,
                                };

                                if contains_between_boundaries(key.as_str(), ':') || key.as_str().contains('/') {
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
                                    )?;

                                    if let Some(prefix_definition) = active_context.get(compact_iri.prefix()) {
                                        let mut result = String::new();

                                        if let Some(prefix_key) = prefix_definition.value() {
                                            if let Some(prefix_iri) = prefix_key.as_iri() {
                                                if let Some(iri) = env.vocabulary.iri(prefix_iri) {
                                                    result = iri.to_string()
                                                }
                                            }
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
                                                        Some(arc) if matches!(arc.as_ref(), Term::Id(Id::Valid(ValidId::Iri(_)))) => definition.value = Some(arc),
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
                        )?.as_deref() {
                            Some(Term::Id(Id::Valid(ValidId::Iri(_)))) => (),
                            _ => return Err(Error::InvalidTermDefinition),
                        }

                        definition.index = Some(index_value.to_owned())
                    }

                    if let Some(context) = value.context {
                        if options.processing_mode == ProcessingMode::JsonLd1_0 {
                            return Err(Error::InvalidTermDefinition);
                        }

                        process_context_sync(env, active_context, context, remote_contexts.clone(), base_url.clone(), options.with_override())
                            .map_err(|_| Error::InvalidScopedContext)?;

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

                    if !options.override_protected {
                        if let Some(previous_definition) = previous_definition {
                            if previous_definition.protected {
                                if definition.modulo_protected_field() != previous_definition.modulo_protected_field() {
                                    return Err(Error::ProtectedTermRedefinition);
                                }

                                definition.protected = true;
                            }
                        }
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

/// Sync mirror of [`super::process_context`].
///
/// Returns `Err(Error::LoadingDocumentFailed)` if a remote `@context` IRI or
/// `@import` is encountered. Pre-scan via [`requires_loader`] before calling.
pub(crate) fn process_context_sync<'l: 'a, 'a, N, L, W>(
    mut env: Environment<'a, N, L, W>,
    active_context: &'a Context<N::Iri, N::BlankId>,
    local_context: &'l syntax::context::Context,
    remote_contexts: ProcessingStack<N::Iri>,
    base_url: Option<N::Iri>,
    mut options: Options,
) -> Result<crate::Processed<'l, N::Iri, N::BlankId>, Error<L::Error>>
where
    N: VocabularyMut,
    N::Iri: Clone + Eq + Hash,
    N::BlankId: Clone + PartialEq,
    L: Loader,
    W: WarningHandler<N>,
{
    let mut result = active_context.clone();

    if let syntax::context::Context::One(syntax::ContextEntry::Definition(def)) = local_context {
        if let Some(propagate) = def.propagate {
            if options.processing_mode == ProcessingMode::JsonLd1_0 {
                return Err(Error::InvalidContextEntry);
            }

            options.propagate = propagate
        }
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

                    if !options.propagate {
                        result.set_previous_context(previous_result);
                    }
                }
            }

            // The pre-scan promised no IriRef. If one is here, fall back via
            // an explicit error rather than silently doing the wrong thing.
            syntax::ContextEntry::IriRef(_) => {
                return Err(Error::LoadingDocumentFailed);
            }

            syntax::ContextEntry::Definition(context) => {
                if context.version.is_some() && options.processing_mode == ProcessingMode::JsonLd1_0 {
                    return Err(Error::ProcessingModeConflict);
                }

                // Pre-scan promised no @import.
                if context.import.is_some() {
                    return Err(Error::LoadingDocumentFailed);
                }

                let context = Merged::new(context, None);

                if remote_contexts.is_empty() {
                    if let Some(value) = context.base() {
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
                }

                if let Some(value) = context.vocab() {
                    match value {
                        syntax::Nullable::Null => {
                            result.set_vocabulary(None);
                        }
                        syntax::Nullable::Some(value) => match expand_iri_simple(&mut env, &result, Nullable::Some(value.into()), true, Some(options.vocab))? {
                            Some(arc) if matches!(arc.as_ref(), Term::Id(_)) => {
                                let term = Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone());
                                result.set_vocabulary(Some(term));
                            }
                            _ => return Err(Error::InvalidVocabMapping),
                        },
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
                    )?
                }
            }
        }
    }

    Ok(crate::Processed::new(local_context, result))
}
