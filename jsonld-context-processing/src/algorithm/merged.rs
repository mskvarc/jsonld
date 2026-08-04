use iri_rs::IriRefBuf;
use jsonld_syntax as syntax;
use syntax::Nullable;

/// A context definition viewed together with the definition it pulls in through
/// `@import`.
///
/// The importing definition wins over the imported one for every entry, which is
/// what the [specification][1] means by "merging context into import context,
/// replacing common entries". Reading through this type lets the algorithm treat
/// the pair as one definition without building a merged copy.
///
/// [1]: https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm
pub struct Merged<'a> {
    base: &'a syntax::context::Definition,
    imported: Option<syntax::context::Context>,
}

impl<'a> Merged<'a> {
    /// Views `base` together with the context it imports.
    ///
    /// `imported` is the already-dereferenced `@import` target, or `None` when
    /// the definition has no `@import`.
    pub fn new(base: &'a syntax::context::Definition, imported: Option<syntax::context::Context>) -> Self {
        Self { base, imported }
    }

    /// Returns the imported context definition, if there is one.
    ///
    /// Yields `None` when nothing was imported, and also when what was imported
    /// is not a single context definition object — a shape the caller has
    /// already rejected before reaching here.
    pub fn imported(&self) -> Option<&syntax::context::Definition> {
        self.imported.as_ref().and_then(|imported| match imported {
            syntax::context::Context::One(syntax::ContextEntry::Definition(import_context)) => Some(import_context),
            _ => None,
        })
    }

    /// Returns the `@base` entry, falling back to the imported definition's.
    pub fn base(&self) -> Option<syntax::Nullable<&IriRefBuf>> {
        self.base
            .base
            .as_ref()
            .or_else(|| self.imported().and_then(|i| i.base.as_ref()))
            .map(Nullable::as_ref)
    }

    /// Returns the `@vocab` entry, falling back to the imported definition's.
    pub fn vocab(&self) -> Option<syntax::Nullable<&syntax::context::definition::Vocab>> {
        self.base
            .vocab
            .as_ref()
            .or_else(|| self.imported().and_then(|i| i.vocab.as_ref()))
            .map(Nullable::as_ref)
    }

    /// Returns the `@language` entry, falling back to the imported definition's.
    pub fn language(&self) -> Option<syntax::Nullable<&syntax::LenientLangTagBuf>> {
        self.base
            .language
            .as_ref()
            .or_else(|| self.imported().and_then(|i| i.language.as_ref()))
            .map(Nullable::as_ref)
    }

    /// Returns the `@direction` entry, falling back to the imported
    /// definition's.
    pub fn direction(&self) -> Option<syntax::Nullable<syntax::Direction>> {
        self.base.direction.or_else(|| self.imported().and_then(|i| i.direction))
    }

    /// Returns the `@protected` entry, falling back to the imported
    /// definition's.
    ///
    /// This is the context-wide default applied to every term it defines, not
    /// the flag of an individual term definition.
    pub fn protected(&self) -> Option<bool> {
        self.base.protected.or_else(|| self.imported().and_then(|i| i.protected))
    }

    /// Returns the `@type` entry, falling back to the imported definition's.
    ///
    /// `@type` is the one keyword a context may redefine, to give it an `@set`
    /// container or mark it protected.
    pub fn type_(&self) -> Option<syntax::context::definition::Type> {
        self.base.type_.or_else(|| self.imported().and_then(|i| i.type_))
    }

    /// Iterates over every term definition of both definitions, without
    /// repeating a term the importing definition overrides.
    pub fn bindings(&self) -> MergedBindings<'_> {
        MergedBindings {
            base: self.base,
            base_bindings: self.base.bindings.iter(),
            imported_bindings: self.imported().map(|i| i.bindings.iter()),
        }
    }

    /// Returns the entry bound to `key`, preferring the importing definition's.
    pub fn get(&self, key: &syntax::context::definition::KeyOrKeyword) -> Option<syntax::context::definition::EntryValueRef<'_>> {
        self.base.get(key).or_else(|| self.imported().and_then(|i| i.get(key)))
    }
}

impl<'a> From<&'a syntax::context::Definition> for Merged<'a> {
    fn from(base: &'a syntax::context::Definition) -> Self {
        Self { base, imported: None }
    }
}

type BindingRef<'a> = (&'a syntax::context::definition::Key, Nullable<&'a syntax::context::TermDefinition>);

/// Iterator over the term definitions of a [`Merged`] context definition.
///
/// Yields the imported definition's terms first, skipping any the importing
/// definition also defines, then the importing definition's own terms. Every
/// term therefore appears exactly once, bound to the definition that wins.
pub struct MergedBindings<'a> {
    base: &'a syntax::context::Definition,
    base_bindings: syntax::context::definition::BindingsIter<'a>,
    imported_bindings: Option<syntax::context::definition::BindingsIter<'a>>,
}

impl<'a> Iterator for MergedBindings<'a> {
    type Item = BindingRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.imported_bindings {
            Some(imported_bindings) => {
                for (key_ref, def) in imported_bindings {
                    let key = key_ref.to_owned();
                    if self.base.get_binding(&key).is_none() {
                        return Some((key_ref, def));
                    }
                }

                self.base_bindings.next()
            }
            None => self.base_bindings.next(),
        }
    }
}
