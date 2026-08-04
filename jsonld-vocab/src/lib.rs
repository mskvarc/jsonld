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
//! # Requirements
//!
//! The generated code references `::iri_rs` (the [`iri-rs`] crate) with its
//! `static` feature enabled, so the **calling crate** must depend on it:
//!
//! ```toml
//! [dependencies]
//! iri-rs = { version = "3", features = ["static"] }
//! ```
//!
//! If you use the umbrella `jsonld` crate with its `vocab` feature you do not
//! need a direct `iri-rs` dependency — point the generated code at the
//! facade's re-export instead with the `iri_crate` field:
//!
//! ```ignore
//! mod vocab {
//!     jsonld::vocab! {
//!         contexts: ["contexts/core.jsonld"],
//!         iri_crate: "jsonld::iri_rs",
//!     }
//! }
//! ```
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
use syn::parse_macro_input;

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
/// for relative paths — a test-harness escape hatch, not rebuild-tracked.)
///
/// The optional `iri_crate` field overrides the path the generated constants
/// use to reach the `iri_rs` crate (default `::iri_rs`); see the crate-level
/// docs for the umbrella-crate setup.
#[proc_macro]
pub fn generate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    expand(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

fn expand(input: MacroInput) -> syn::Result<proc_macro2::TokenStream> {
    let contexts = InputContexts::resolve(&input)?;
    let resolved = load_contexts(&contexts)?;

    let iri_crate = match &input.iri_crate {
        Some(path) => quote::quote!(#path),
        None => quote::quote!(::iri_rs),
    };

    generate_tokens(&resolved, &iri_crate)
}
