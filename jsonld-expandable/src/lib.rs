//! Derive macro generating expanded JSON-LD directly from Rust types.
//!
//! All parsing, IR, and codegen logic lives in `jsonld-expandable-core` so
//! it can be unit-tested without `trybuild`.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derive `Expandable` to generate an `expand::<V>()` impl producing expanded
/// JSON-LD for any `V: JsonValue` backend.
///
/// ```ignore
/// use jsonld_expandable::Expandable;
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
/// | `crate = "path"` | Path of the runtime crate in generated code. Defaults to `::jsonld_expandable_core`, which only resolves for crates that depend on the core crate directly — **users of the umbrella `jsonld` crate must set `#[jsonld(crate = "jsonld::expandable_core")]`**. |
/// | `debug` | Report the generated code as a compile error (for inspection). |
///
/// Exactly one of `type = "..."`, a `type_value` field, or `fragment` is
/// required.
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
/// | `container = "language"` | Language map via the `ExpandableLanguageMap` trait. |
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
    jsonld_expandable_core::derive_expandable(ast).into()
}
