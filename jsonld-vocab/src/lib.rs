//! Proc-macro that generates JSON-LD vocabulary constants from local context files.
//!
//! At compile time the macro reads one or more local JSON-LD context files,
//! resolves their prefixes and terms offline, and emits typed
//! `&'static iri_rs::Iri` constants together with string lookup tables.
//!
//! Terms are partitioned by JSON-LD casing convention into
//! [`classes`](self) (TitleCase) and [`properties`](self) (lowerCase)
//! submodules, so a context can declare both `Property` and `property`
//! without colliding on a single Rust constant name.

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
/// ```ignore
/// jsonld_vocab::generate! {
///     contexts: ["contexts/core.jsonld", "contexts/extras.jsonld"]
/// }
/// ```
///
/// Paths are resolved relative to the calling crate's `CARGO_MANIFEST_DIR`.
#[proc_macro]
pub fn generate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    expand(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

fn expand(input: MacroInput) -> syn::Result<proc_macro2::TokenStream> {
    let contexts = InputContexts::resolve(input)?;
    let resolved = load_contexts(&contexts)?;

    generate_tokens(&contexts, &resolved)
}
