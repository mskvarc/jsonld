//! Derive macro generating expanded JSON-LD directly from Rust types.
//!
//! All parsing, IR, and codegen logic lives in `jsonld-expandable-core` so
//! it can be unit-tested without `trybuild`.

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_macro_input};

/// Resolves the path the generated code uses to reach the runtime crate.
///
/// The generated `impl` is compiled inside the consuming crate, so the path has
/// to name a crate that crate depends on. That is either
/// `jsonld-expandable-core` directly, giving `::jsonld_expandable_core`, or the
/// umbrella `jsonld` crate with an `expandable*` feature, giving
/// `::jsonld::expandable_core`. Either may be renamed in `Cargo.toml`.
fn runtime_path(span: proc_macro2::Span) -> syn::Result<TokenStream2> {
    if let Ok(found) = crate_name("jsonld-expandable-core") {
        return Ok(match found {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let ident = format_ident!("{}", name);
                quote!(::#ident)
            }
        });
    }

    match crate_name("jsonld") {
        Ok(FoundCrate::Itself) => Ok(quote!(crate::expandable_core)),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            Ok(quote!(::#ident::expandable_core))
        }
        Err(_) => Err(syn::Error::new(
            span,
            "`Expandable` needs its runtime crate as a dependency: add `jsonld` with an \
             `expandable` feature, or `jsonld-expandable-core` directly",
        )),
    }
}

/// Derive `Expandable` to generate an `expand::<V>()` impl producing expanded
/// JSON-LD for any `V: JsonValue` backend.
///
/// ```ignore
/// // The derive macro, plus the trait it implements: `expand()` is a trait
/// // method, so both names must be in scope. Deriving through the umbrella
/// // `jsonld` crate, a single `use jsonld::Expandable;` brings both.
/// use jsonld_expandable::Expandable;
/// use jsonld_expandable_core::Expandable as _;
///
/// #[derive(Expandable)]
/// #[jsonld(type = "https://example.com/Parent")]
/// pub struct Parent {
///     #[jsonld(id)]
///     pub id: String,
///
///     #[jsonld(property = "https://example.com/name")]
///     pub name: String,
/// }
///
/// let expanded: serde_json::Value = parent.expand();
/// ```
///
/// # Container attributes
///
/// Set on the struct itself, inside `#[jsonld(...)]`:
///
/// | Attribute | Meaning |
/// |---|---|
/// | `type = "IRI-or-CURIE"` | Static `@type` for every expanded node. |
/// | `fragment` | Emit the object without `@type` (a sub-fragment of a parent node). Mutually exclusive with `type` and a `type_value` field. |
/// | `prefix(name = "IRI", ...)` | Declare CURIE prefixes used by `type`, `property`, and `coerce` values. Hyphenated names work bare (`prefix(ngsi-ld = "...")`) or quoted (`prefix("ngsi-ld" = "...")`). |
/// | `debug` | Report the generated code as a compile error (for inspection). |
///
/// Exactly one of `type = "..."`, a `type_value` field, or `fragment` is
/// required.
///
/// The generated code reaches its runtime support through
/// `jsonld-expandable-core` or through the umbrella `jsonld` crate's re-export
/// of it, whichever the consuming crate depends on, following a renamed
/// dependency to the name it was given.
///
/// # Field attributes
///
/// | Attribute | Meaning |
/// |---|---|
/// | `id` | Field is the node's `@id`. `String`-like (`AsRef<str>`) or `Option` thereof; `None` omits the `@id` entry. |
/// | `type_value` | Field supplies a dynamic `@type` array via the `ExpandableTypeValue` trait. At most one per struct; conflicts with container `type`/`fragment`. |
/// | `property = "IRI-or-CURIE"` | Property IRI the field expands under. Required unless the field is `id`, `skip`, `flatten`, or `flatten_map`. |
/// | `skip` | Exclude the field from expansion. |
/// | `coerce = "@id"` / `"@vocab"` | Value is an IRI reference: emits `[{"@id": v}]` (or `@vocab`). With `vec`, iterates the field. |
/// | `coerce = "@json"` | JSON literal: emits `[{"@value": v, "@type": "@json"}]`. |
/// | `coerce = "IRI-or-CURIE"` | Typed literal: emits `[{"@value": v, "@type": "<datatype>"}]`. CURIEs expand through the container's `prefix(...)` table. |
/// | `container = "list"` | Wrap values as `[{"@list": [...]}]`. Composes with `coerce` and `nested` for per-item shapes. |
/// | `container = "set"` | Emit the value's `ToJsonValue` form verbatim (JSON array). |
/// | `container = "language"` | Language map: `HashMap`/`BTreeMap` of tag to text work as they are, other shapes implement `ExpandableLanguageMap`. |
/// | `container = "index"` | Index map: emits `[{"@index": k, "@value": v}, ...]` from an iterable of pairs. |
/// | `nested` | Field is itself `Expandable`; recurse. With `vec`, expands each element. |
/// | `vec` | Field is a `Vec`-like iterable; combine with `nested` or an `@id`/`@vocab` coercion. |
/// | `flatten` | Merge the expanded properties of a nested `Expandable` into this node (drops its `@id`/`@type`). |
/// | `flatten_map` | Merge a map field: each key becomes a property IRI, each value is expanded recursively. |
/// | `passthrough` | The field's own `Expandable` impl produces the final JSON-LD form; emitted verbatim. |
///
/// `Option<T>` fields omit their entry when `None`.
///
/// # Limitations
///
/// Generic structs are not supported: the derive cannot yet add the
/// `T: ToJsonValue<V>`-style bounds the generated impl would need. Reserved
/// attributes `nest`, `reverse`, and `container = "id"/"type"/"graph"` fail
/// with an explicit error.
#[proc_macro_derive(Expandable, attributes(jsonld))]
pub fn derive_expandable(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    match runtime_path(ast.ident.span()) {
        Ok(runtime) => jsonld_expandable_core::derive_expandable(ast, &runtime).into(),
        Err(err) => err.to_compile_error().into(),
    }
}
