//! Proc-macro shell. Forwards to `jsonld_expandable_core::derive_expandable`.
//!
//! All parsing, IR, and codegen logic lives in `jsonld-expandable-core` so
//! it can be unit-tested without `trybuild`.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derive `Expandable` to generate an `expand::<V>()` impl producing expanded
/// JSON-LD for any `V: JsonValue` backend.
///
/// See the `jsonld-expandable-core` crate docs for the attribute model.
#[proc_macro_derive(Expandable, attributes(jsonld))]
pub fn derive_expandable(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    jsonld_expandable_core::derive_expandable(ast).into()
}
