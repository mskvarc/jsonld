//! Proc-macro that generates JSON-LD vocabulary constants from local context files.
//!
//! At compile time the macro reads one or more local JSON-LD context files,
//! resolves their prefixes and terms offline, and emits typed
//! `iri_rs::Iri<&'static str>` constants together with string lookup tables.
//!
//! Terms are partitioned by JSON-LD casing convention into
//! `classes` (TitleCase) and `properties` (lowerCase)
//! submodules, so a context can declare both `Property` and `property`
//! without colliding on a single Rust constant name.
//!
//! # Casing assumptions
//!
//! The partition is a convention the specification does not mandate, the one
//! NGSI-LD and most RDF vocabularies follow: a term whose first character is
//! uppercase is a class, everything else is a property. Contexts that name
//! things some other way still get a constant per term and complete lookup
//! tables, but the module names stop describing their contents, and terms with
//! no cased characters at all (CJK names, for instance) all count as
//! properties.
//!
//! Constant names are the term in SHOUTY_SNAKE_CASE, so terms differing only in
//! how they mark word boundaries collapse onto one name. `createdAt` and
//! `created_at` both want `CREATED_AT`, and since the class/property split
//! cannot separate two terms from the same side of it, that is a naming
//! collision error rather than generated code. A context consistent about
//! either style is fine; one that mixes both for the same concept cannot be
//! used here.
//!
//! # Requirements
//!
//! The calling crate needs [`iri-rs`] as a dependency:
//!
//! ```toml
//! [dependencies]
//! iri-rs = { version = "3", features = ["static"] }
//! ```
//!
//! This cannot be routed through another crate's re-export of `iri-rs`. The
//! constants are built by `iri-rs`'s `iri!` macro, which resolves `iri-rs`
//! against the calling crate's own manifest when it expands, so naming a
//! re-exported path fails inside that macro. Renaming the dependency is fine:
//! this macro looks the name up rather than assuming `iri_rs`.
//!
//! Depending on the umbrella `jsonld` crate with its `vocab` feature covers the
//! `static` part, since features unify, so a plain `iri-rs = "3"` alongside it
//! is enough.
//!
//! [`iri-rs`]: https://docs.rs/iri-rs
mod codegen;
mod model;
mod resolve;

use crate::{
    codegen::generate_tokens,
    model::{InputContexts, MacroInput},
    resolve::load_contexts,
};
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse_macro_input;

/// Resolves the path the generated constants use to reach `iri-rs`.
///
/// The constants are built by `iri-rs`'s own `iri!` macro, which resolves
/// `iri-rs` against the calling crate's manifest when it expands. Reaching it
/// through another crate's re-export therefore cannot work, and `iri-rs` has to
/// be a dependency of the calling crate. Looking the name up here means a
/// renamed dependency still works, and a missing one gets an error naming the
/// macro that needs it.
fn iri_path(span: Span) -> syn::Result<proc_macro2::TokenStream> {
    match crate_name("iri-rs") {
        Ok(FoundCrate::Itself) => Ok(quote!(crate)),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            Ok(quote!(::#ident))
        }
        Err(_) => Err(syn::Error::new(
            span,
            "the generated constants are `iri-rs` types, so the calling crate needs \
             `iri-rs = { version = \"3\", features = [\"static\"] }` as a dependency",
        )),
    }
}

/// Generate JSON-LD vocabulary constants from one or more local context files.
///
/// Wrap each invocation in its own module: the macro emits fixed names
/// (`prefix`, `expanded`, `compact`, `TERM_COUNT`, `COMPACT_TO_EXPANDED`,
/// `EXPANDED_TO_COMPACT`, lookup functions), so two invocations in one module
/// collide.
///
/// ```ignore
/// mod vocab {
///     jsonld_vocab::generate! {
///         contexts: ["contexts/core.jsonld", "contexts/extras.jsonld"]
///     }
/// }
///
/// let iri: iri_rs::Iri<&'static str> = vocab::expanded::properties::CREATED_AT;
/// ```
///
/// Relative paths are resolved against the calling crate's
/// `CARGO_MANIFEST_DIR`; absolute paths are used as-is. (The
/// `JSONLD_VOCAB_BASE_DIR` environment variable overrides the base directory
/// for relative paths, a test-harness escape hatch that is not rebuild-tracked.)
///
/// `contexts` is the only field. See the crate-level docs for the `iri-rs`
/// dependency the generated constants are built from.
#[proc_macro]
pub fn generate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    expand(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

fn expand(input: MacroInput) -> syn::Result<proc_macro2::TokenStream> {
    let contexts = InputContexts::resolve(&input)?;
    let resolved = load_contexts(&contexts)?;
    let iri_crate = iri_path(input.contexts_span)?;

    generate_tokens(&resolved, &iri_crate)
}
