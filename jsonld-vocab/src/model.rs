use proc_macro2::Span;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, path::PathBuf};
use syn::{
    LitStr,
    Result,
    Token,
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Parsed macro input.
pub struct MacroInput {
    /// Ordered list of local context paths.
    pub contexts: Vec<LitStr>,
    /// Span pointing at the `contexts` field for diagnostics.
    pub contexts_span: Span,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut contexts: Option<Vec<LitStr>> = None;
        let mut contexts_span: Option<Span> = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            if key == "contexts" {
                let span = key.span();
                let bracketed_content;
                bracketed!(bracketed_content in input);
                let punct: Punctuated<LitStr, Token![,]> = Punctuated::parse_terminated(&bracketed_content)?;
                contexts_span = Some(span);
                contexts = Some(punct.into_iter().collect());
            } else {
                return Err(syn::Error::new(key.span(), format!("unknown field `{key}`, expected `contexts`")));
            }

            let _ = input.parse::<Option<Token![,]>>()?;
        }

        let contexts_span = contexts_span.ok_or_else(|| syn::Error::new(Span::call_site(), "missing required field `contexts`"))?;
        let contexts = contexts.unwrap_or_default();

        if contexts.is_empty() {
            return Err(syn::Error::new(contexts_span, "`contexts` must contain at least one path"));
        }

        Ok(Self { contexts, contexts_span })
    }
}

/// A resolved input context file.
#[derive(Clone, Debug)]
pub struct InputContextFile {
    /// Resolved absolute path to the JSON-LD file.
    pub resolved_path: PathBuf,
    /// Literal span for diagnostics.
    pub span: Span,
}

/// All resolved input contexts.
#[derive(Clone, Debug)]
pub struct InputContexts {
    /// Files in merge order.
    pub files: Vec<InputContextFile>,
}

impl InputContexts {
    /// Resolve the macro input using paths relative to `CARGO_MANIFEST_DIR`.
    /// Absolute paths (POSIX or Windows drive paths) are used as-is.
    ///
    /// The `JSONLD_VOCAB_BASE_DIR` environment variable, if set, overrides
    /// the base directory for relative paths. This exists as a test escape
    /// hatch so trybuild drivers can resolve paths against the originating
    /// crate root rather than the trybuild wip directory. Note that proc
    /// macros cannot register environment variables for rebuild tracking:
    /// changing it does not by itself invalidate an existing build.
    pub fn resolve(input: &MacroInput) -> Result<InputContexts> {
        let base_dir = std::env::var("JSONLD_VOCAB_BASE_DIR")
            .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
            .map_err(|error| syn::Error::new(input.contexts_span, format!("CARGO_MANIFEST_DIR is not set: {error}")))?;
        let base = PathBuf::from(base_dir);

        let files = input
            .contexts
            .iter()
            .map(|lit| {
                let raw = PathBuf::from(lit.value());
                let resolved_path = if raw.is_absolute() { raw } else { base.join(raw) };
                InputContextFile {
                    resolved_path,
                    span: lit.span(),
                }
            })
            .collect();

        Ok(Self { files })
    }

    /// Iterate over the files in merge order.
    pub fn iter(&self) -> impl Iterator<Item = &InputContextFile> {
        self.files.iter()
    }
}

/// A parsed JSON-LD context document.
#[derive(Clone, Debug)]
pub struct ParsedDocument {
    /// Metadata about the source file.
    pub input: InputContextFile,
    /// Parsed root JSON value.
    pub root: Value,
}

impl ParsedDocument {
    /// Return the top-level `@context` object.
    pub fn top_level_context(&self) -> syn::Result<&Map<String, Value>> {
        self.root.get("@context").and_then(Value::as_object).ok_or_else(|| {
            syn::Error::new(
                self.input.span,
                format!("context {} is missing a top-level @context object", self.input.resolved_path.display()),
            )
        })
    }
}

/// Source information for a discovered prefix or term.
#[derive(Clone, Debug)]
pub struct DefinitionSource {
    /// Source file path.
    pub path: PathBuf,
    /// Span of the context-path literal the definition came from.
    pub span: Span,
}

/// Prefix definition extracted from the JSON-LD contexts.
#[derive(Clone, Debug)]
pub struct PrefixDefinition {
    /// Compact prefix key.
    pub compact: String,
    /// Expanded namespace IRI.
    pub expanded: String,
    /// Definition origin.
    pub source: DefinitionSource,
}

/// How a term's `@id` was declared before resolution.
#[derive(Clone, Debug)]
pub enum TermTarget {
    /// Absolute IRI.
    Absolute(String),
    /// Prefixed name such as `ngsi-ld:Property`.
    Prefixed { prefix: String, suffix: String },
    /// Local compact term reference.
    LocalReference(String),
}

/// Term definition extracted from the JSON-LD contexts.
#[derive(Clone, Debug)]
pub struct TermDefinition {
    /// Compact term key.
    pub compact: String,
    /// Unresolved identity target.
    pub target: TermTarget,
    /// Definition origin.
    pub source: DefinitionSource,
}

/// Resolved prefix constant data.
#[derive(Clone, Debug)]
pub struct ResolvedPrefix {
    /// Compact prefix key.
    pub compact: String,
    /// Expanded namespace IRI.
    pub expanded: String,
    /// Rust constant name.
    pub const_name: String,
}

/// Resolved term constant data.
#[derive(Clone, Debug)]
pub struct ResolvedTerm {
    /// Compact term key.
    pub compact: String,
    /// Expanded term IRI.
    pub expanded: String,
    /// Rust constant name.
    pub const_name: String,
}

/// Final resolved vocabulary model.
#[derive(Clone, Debug)]
pub struct ResolvedModel {
    /// Resolved prefixes.
    pub prefixes: Vec<ResolvedPrefix>,
    /// Class-like terms (TitleCase compact names).
    pub class_terms: Vec<ResolvedTerm>,
    /// Property-like terms (lowerCase compact names).
    pub property_terms: Vec<ResolvedTerm>,
    /// Absolute paths to include for rebuild tracking.
    pub include_paths: Vec<String>,
}

/// Mutable model built while scanning all input contexts.
#[derive(Clone, Debug, Default)]
pub struct ContextModel {
    /// Prefixes keyed by compact prefix.
    pub prefixes: BTreeMap<String, PrefixDefinition>,
    /// Terms keyed by compact term.
    pub terms: BTreeMap<String, TermDefinition>,
}
