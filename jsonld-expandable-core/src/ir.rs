//! Intermediate representation that the attribute parser produces and the
//! codegen consumes. Keeping this typed and separate from the `syn` AST makes
//! it possible to unit-test the parser and snapshot codegen output without
//! reaching for `trybuild`.

use proc_macro2::TokenStream;

#[derive(Debug, Default)]
/// Container a field's values are laid out in.
pub struct ContainerIr {
    /// Static `@type` IRI. Mutually exclusive with `type_field` and `fragment`.
    pub type_iri: Option<String>,
    /// Field marked `#[jsonld(type_value)]` providing a dynamic `@type`.
    pub type_field: Option<syn::Ident>,
    /// Emit object without `@type` (sub-fragment).
    pub fragment: bool,
    /// Override of the runtime crate path (default `::jsonld_expandable_core`).
    pub crate_path: Option<TokenStream>,
    /// Local CURIE prefix table.
    pub prefixes: Vec<(String, String)>,
    /// Print generated code as a compile error.
    pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Coercion applied to a field's value.
pub enum Coerce {
    /// Coerce to `@id`.
    Id,
    /// Coerce to `@vocab`.
    Vocab,
    /// A JSON literal.
    Json,
    /// Coerce to the given datatype IRI.
    Datatype(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Kind of container declared on a field.
pub enum ContainerKind {
    /// The `@list` entry, marking the values as an ordered list.
    List,
    /// The `@set` entry, marking the values as an unordered set.
    Set,
    /// The `@language` entry, tagging string values with a language.
    Language,
    /// The `@index` entry, indexing the value within its container.
    Index,
    /// The `@id` entry, identifying the node or mapping the term to an IRI.
    Id,
    /// The `@type` entry, giving the type of the node or the values.
    Type,
    /// The `@graph` entry, holding the node objects of a named graph.
    Graph,
}

#[derive(Debug, Default)]
/// Everything the derive learned about one field.
pub struct FieldIr {
    /// Field is the `@id` of the surrounding node.
    pub is_id: bool,
    /// Field provides the dynamic `@type` (matches container.type_field).
    pub is_type_value: bool,
    /// Skip this field entirely.
    pub skip: bool,
    /// Property IRI (or CURIE pre-expansion).
    pub property: Option<String>,
    /// Coercion mode.
    pub coerce: Option<Coerce>,
    /// Container mode.
    pub container: Option<ContainerKind>,
    /// Field is a nested `Expandable`.
    pub nested: bool,
    /// Field is a `Vec<_>` (used with `nested`).
    pub is_vec: bool,
    /// Merge expanded properties of a nested `Expandable` into the parent
    /// (drops `@id` / `@type`). Field type must implement `Expandable`.
    pub flatten: bool,
    /// Merge a map (`HashMap<K, V>` / `IndexMap<K, V>`) into the parent
    /// where each entry's key becomes a property and each value is the
    /// recursively expanded `V`.
    pub flatten_map: bool,
    /// Field's `Expandable` impl (manual or derived) produces the final
    /// JSON-LD form for this property — emit it verbatim, no wrapping in
    /// `[{"@value": ...}]` or `[obj]`. Legacy spelling: `#[jsonld(custom)]`.
    pub passthrough: bool,
}
