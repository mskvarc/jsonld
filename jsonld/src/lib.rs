//! This crate is a Rust implementation of the
//! [JSON-LD](https://www.w3.org/TR/json-ld/)
//! data interchange format.
//!
//! [Linked Data (LD)](https://www.w3.org/standards/semanticweb/data)
//! is a [World Wide Web Consortium (W3C)](https://www.w3.org/)
//! initiative built upon standard Web technologies to create an
//! interrelated network of datasets across the Web.
//! The [JavaScript Object Notation (JSON)](https://tools.ietf.org/html/rfc7159) is
//! a widely used, simple, unstructured data serialization format to describe
//! data objects in a human readable way.
//! JSON-LD brings these two technologies together, adding semantics to JSON
//! to create a lightweight data serialization format that can organize data and
//! help Web applications to inter-operate at a large scale.
//!
//! # Usage
//!
//! The entry point for this library is the [`JsonLdProcessor`] trait
//! that provides an access to all the JSON-LD transformation algorithms
//! (context processing, expansion, compaction, etc.).
//! If you want to explore and/or transform [`ExpandedDocument`]s, you may also
//! want to check out the [`Object`] type representing a JSON object.
//!
//! [`JsonLdProcessor`]: crate::JsonLdProcessor
//!
//! ## Expansion
//!
//! If you want to expand a JSON-LD document, first describe the document to
//! be expanded using either [`RemoteDocument`] or [`RemoteDocumentReference`]:
//!   - [`RemoteDocument`] wraps the JSON representation of the document
//!     alongside its remote URL.
//!   - [`RemoteDocumentReference`] may represent only an URL, letting
//!     some loader fetching the remote document by dereferencing the URL.
//!
//! After that, you can simply use the [`JsonLdProcessor::expand`] function on
//! the remote document.
//!
//! [`RemoteDocument`]: crate::RemoteDocument
//! [`RemoteDocumentReference`]: crate::RemoteDocumentReference
//! [`JsonLdProcessor::expand`]: JsonLdProcessor::expand
//!
//! ### Example
//!
//! ```
//! use iri_rs::{iri, IriBuf};
//! use jsonld::{JsonLdProcessor, Options, RemoteDocument, syntax::{Value, Parse}};
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create a "remote" document by parsing a file manually.
//! let input = RemoteDocument::new(
//!   // We use `IriBuf` as IRI type.
//!   Some(IriBuf::from(iri!("https://example.com/sample.jsonld"))),
//!
//!   // Optional content type.
//!   Some("application/ld+json".parse().unwrap()),
//!   
//!   // Parse the file.
//!   Value::parse_str(r#"
//!     {
//!       "@context": {
//!         "name": "http://xmlns.com/foaf/0.1/name"
//!       },
//!       "@id": "https://www.rust-lang.org",
//!       "name": "Rust Programming Language"
//!     }"#).expect("unable to parse file").0
//! );
//!
//! // Use `NoLoader` as we won't need to load any remote document.
//! let mut loader = jsonld::NoLoader;
//!
//! // Expand the "remote" document.
//! let expanded = input
//!   .expand(&mut loader)
//!   .await
//!   .expect("expansion failed");
//!
//! for object in expanded {
//!   if let Some(id) = object.id() {
//!     let name = object.as_node().unwrap()
//!       .get_any(&iri!("http://xmlns.com/foaf/0.1/name")).unwrap()
//!       .as_str().unwrap();
//!
//!     println!("id: {id}");
//!     println!("name: {name}");
//!   }
//! }
//! # }
//! ```
//!
//! Here is another example using `RemoteDocumentReference`.
//!
//! ```
//! use iri_rs::{iri, IriBuf};
//! use jsonld::{JsonLdProcessor, Options, RemoteDocumentReference};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let input = RemoteDocumentReference::iri(IriBuf::from(iri!("https://example.com/sample.jsonld")));
//!
//! // Use `FsLoader` to redirect any URL starting with `https://example.com/` to
//! // the local `example` directory. No HTTP query.
//! let mut loader = jsonld::FsLoader::default();
//! loader.mount(IriBuf::from(iri!("https://example.com/")), "examples");
//!
//! let expanded = input.expand(&mut loader)
//!   .await
//!   .expect("expansion failed");
//! # }
//! ```
//!
//! Lastly, the same example replacing [`IriBuf`] with the lightweight
//! [`rdfx::vocabulary::IriIndex`] type.
//!
//!
//! ```
//! # use iri_rs::{iri, IriBuf};
//! # use jsonld::{JsonLdProcessor, Options, RemoteDocumentReference};
//! use rdfx::vocabulary::{IriVocabularyMut, IndexVocabulary};
//! use jsonld::Id;
//! use contextual::WithContext;
//! # #[tokio::main]
//! # async fn main() {
//! // Creates the vocabulary that will map each `rdfx::vocabulary::Index`
//! // to an actual `IriBuf`.
//! let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
//!
//! let iri_index = vocabulary.insert(iri!("https://example.com/sample.jsonld"));
//! let input = RemoteDocumentReference::iri(iri_index);
//!
//! // Use `FsLoader` to redirect any URL starting with `https://example.com/` to
//! // the local `example` directory. No HTTP query.
//! let mut loader = jsonld::FsLoader::default();
//! loader.mount(IriBuf::from(iri!("https://example.com/")), "examples");
//!
//! let expanded = input
//!   .expand_with(&mut vocabulary, &mut loader)
//!   .await
//!   .expect("expansion failed");
//!
//! // `foaf:name` property identifier.
//! let name_id = Id::iri(vocabulary.insert(iri!("http://xmlns.com/foaf/0.1/name")));
//!
//! for object in expanded {
//!   if let Some(id) = object.id() {
//!     let name = object.as_node().unwrap()
//!       .get_any(&name_id).unwrap()
//!       .as_value().unwrap()
//!       .as_str().unwrap();
//!
//!     println!("id: {}", id.with(&vocabulary));
//!     println!("name: {name}");
//!   }
//! }
//! # }
//! ```
//!
//! ## Compaction
//!
//! The JSON-LD Compaction is a transformation that consists in applying a
//! context to a given JSON-LD document reducing its size.
//! There are two ways to get a compact JSON-LD document with this library
//! depending on your starting point:
//!   - If you want to get a compact representation for an arbitrary remote
//!     document, simply use the [`JsonLdProcessor::compact`]
//!     (or [`JsonLdProcessor::compact_with`]) method.
//!   - Otherwise to compact an [`ExpandedDocument`] you can use the
//!     [`Compact::compact`] method.
//!
//! [`JsonLdProcessor::compact`]: crate::JsonLdProcessor::compact
//! [`JsonLdProcessor::compact_with`]: crate::JsonLdProcessor::compact_with
//! [`ExpandedDocument`]: crate::ExpandedDocument
//! [`Compact::compact`]: crate::Compact::compact
//!
//! ### Example
//!
//! Here is an example compacting an arbitrary [`RemoteDocumentReference`]
//! using [`JsonLdProcessor::compact`].
//!
//! ```
//! use iri_rs::{iri, IriBuf};
//! use jsonld::{JsonLdProcessor, Options, RemoteDocumentReference, RemoteContextReference, syntax::Print};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let input = RemoteDocumentReference::iri(IriBuf::from(iri!("https://example.com/sample.jsonld")));
//!
//! let context = RemoteContextReference::iri(IriBuf::from(iri!("https://example.com/context.jsonld")));
//!
//! // Use `FsLoader` to redirect any URL starting with `https://example.com/` to
//! // the local `example` directory. No HTTP query.
//! let mut loader = jsonld::FsLoader::default();
//! loader.mount(IriBuf::from(iri!("https://example.com/")), "examples");
//!
//! let compact = input
//!   .compact(context, &mut loader)
//!   .await
//!   .expect("compaction failed");
//!
//! println!("output: {}", compact.pretty_print());
//! # }
//! ```
//!
//! ## Flattening
//!
//! The JSON-LD Flattening is a transformation that consists in moving nested
//! nodes out. The result is a list of all the nodes declared in the document.
//! There are two ways to flatten JSON-LD document with this library
//! depending on your starting point:
//!   - If you want to get a compact representation for an arbitrary remote
//!     document, simply use the [`JsonLdProcessor::flatten`]
//!     (or [`JsonLdProcessor::flatten_with`]) method.
//!     This will return a JSON-LD document.
//!   - Otherwise to compact an [`ExpandedDocument`] you can use the
//!     [`Flatten::flatten`] (or [`Flatten::flatten_with`]) method.
//!     This will return the list of nodes as a [`FlattenedDocument`].
//!
//! Flattening requires assigning an identifier to nested anonymous nodes,
//! which is why the flattening functions take an [`rdfx::LocalGenerator`]
//! as parameter. This generator is in charge of creating new fresh identifiers
//! (with their metadata). The most common generator is
//! [`rdfx::generator::Blank`] that creates blank node identifiers.
//!
//! [`JsonLdProcessor::flatten`]: crate::JsonLdProcessor::flatten
//! [`JsonLdProcessor::flatten_with`]: crate::JsonLdProcessor::flatten_with
//! [`Flatten::flatten`]: crate::Flatten::flatten
//! [`Flatten::flatten_with`]: crate::Flatten::flatten_with
//! [`FlattenedDocument`]: crate::FlattenedDocument
//!
//! ### Example
//!
//! Here is an example flattening an arbitrary [`RemoteDocumentReference`]
//! using [`JsonLdProcessor::flatten`].
//!
//! ```
//! use iri_rs::{iri, IriBuf};
//! use jsonld::{JsonLdProcessor, Options, RemoteDocumentReference, syntax::Print};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let input = RemoteDocumentReference::iri(IriBuf::from(iri!("https://example.com/sample.jsonld")));
//!
//! // Use `FsLoader` to redirect any URL starting with `https://example.com/` to
//! // the local `example` directory. No HTTP query.
//! let mut loader = jsonld::FsLoader::default();
//! loader.mount(IriBuf::from(iri!("https://example.com/")), "examples");
//!
//! let mut generator = rdfx::generator::Blank::new();
//!
//! let nodes = input
//!   .flatten(&mut generator, &mut loader)
//!   .await
//!   .expect("flattening failed");
//!
//! println!("output: {}", nodes.pretty_print());
//! # }
//! ```
//!
//! ## Interop with other JSON crates
//!
//! This crate represents JSON values as [`jstrict::Value`], whose object type
//! preserves entry order and tolerates duplicate keys — both of which the
//! JSON-LD algorithms depend on. Two feature flags convert to and from the
//! common alternatives without manual unpacking:
//!
//! | Feature | Type it bridges to |
//! |---|---|
//! | `serde-json` | [`serde_json::Value`] |
//! | `sonic-rs` | [`sonic_rs::Value`] |
//!
//! ### Input
//!
//! `RemoteDocument::from_serde_json` and `RemoteDocument::from_sonic_rs`
//! consume the respective value type directly.
//! [`RemoteDocument::from_value`] is more general and accepts anything
//! implementing `Into<jstrict::Value>`.
//!
//! ### Output
//!
//! - [`JsonLdProcessor::compact`] and [`JsonLdProcessor::flatten`] return a
//!   [`jstrict::Value`] — call its inherent `into_serde_json` /
//!   `into_sonic_rs` method.
//! - [`ExpandedDocument`] gains `into_serde_json_with` / `into_sonic_rs_with`,
//!   and `into_serde_json` / `into_sonic_rs` for the default no-vocabulary
//!   case.
//!
//! Note that a round trip through either type is lossy in one respect: neither
//! `serde_json::Map` nor `sonic_rs::Object` keeps duplicate keys, and
//! `sonic_rs::Object` does not preserve entry order.
//!
//! ### Example
//!
//! ```
//! # #[cfg(feature = "serde-json")]
//! # {
//! use iri_rs::{iri, IriBuf};
//! use jsonld::{JsonLdProcessor, RemoteDocument};
//!
//! # use tokio::runtime::Runtime;
//! # Runtime::new().unwrap().block_on(async {
//! let value = serde_json::json!({
//!     "@context": {"name": "http://xmlns.com/foaf/0.1/name"},
//!     "@id": "https://www.rust-lang.org",
//!     "name": "Rust Programming Language"
//! });
//!
//! let input = RemoteDocument::from_serde_json(
//!     Some(IriBuf::from(iri!("https://example.com/sample.jsonld"))),
//!     Some("application/ld+json".parse().unwrap()),
//!     value,
//! );
//!
//! let mut loader = jsonld::NoLoader;
//! let expanded = input.expand(&mut loader).await.expect("expansion failed");
//!
//! let _: serde_json::Value = expanded.into_serde_json();
//! # });
//! # }
//! ```
//!
//! [`serde_json::Value`]: https://docs.rs/serde_json/latest/serde_json/enum.Value.html
//! [`sonic_rs::Value`]: https://docs.rs/sonic-rs/latest/sonic_rs/enum.Value.html
//! [`serde`]: https://docs.rs/serde
//!
//! ## Expanding your own types with `#[derive(Expandable)]`
//!
//! The algorithms above all start from a JSON document. If the data starts out
//! as Rust structs instead, the `expandable` feature lets a type render
//! *directly* into expanded JSON-LD, skipping both the intermediate compact
//! JSON and the expansion algorithm.
//!
//! Annotate the struct with the IRIs its fields map to and derive
//! [`Expandable`]:
//!
//! ```
//! # #[cfg(feature = "expandable-serde-json")]
//! # {
//! use jsonld::Expandable;
//!
//! #[derive(Expandable)]
//! #[jsonld(
//!     type = "https://schema.org/Person",
//!     prefix(schema = "https://schema.org/"),
//!     crate = "jsonld::expandable_core"
//! )]
//! struct Person {
//!     #[jsonld(id)]
//!     id: String,
//!
//!     #[jsonld(property = "schema:name")]
//!     name: String,
//!
//!     #[jsonld(property = "schema:age")]
//!     age: Option<u32>,
//! }
//!
//! let person = Person {
//!     id: "https://example.com/#me".to_owned(),
//!     name: "Ada".to_owned(),
//!     age: Some(36),
//! };
//!
//! let json: serde_json::Value = person.expand();
//! # }
//! ```
//!
//! The derive is backend-agnostic: [`Expandable::expand`] is generic over the
//! [`JsonValue`] trait, so the same type can render into
//! [`serde_json::Value`], `sonic_rs::Value`, or [`jstrict::Value`] depending
//! on which backend feature is enabled. Field attributes cover `@type`
//! coercion, language maps, nesting, flattening and more — see
//! [`Expandable`] for the full attribute reference.
//!
//! **Note:** because the derive macro generates paths into its support crate,
//! users of this umbrella crate must set
//! `#[jsonld(crate = "jsonld::expandable_core")]`, as above.
//!
//! ## Compile-time vocabularies with `vocab!`
//!
//! Repeatedly writing full IRIs is verbose and easy to get wrong. With the
//! `vocab` feature, the [`vocab!`] macro reads one or more JSON-LD context
//! files at compile time and emits a constant for every term they define, so a
//! typo becomes a compile error rather than a silently wrong IRI:
//!
//! ```ignore
//! mod vocab {
//!     jsonld::vocab! {
//!         contexts: ["contexts/schema.jsonld"],
//!         iri_crate: "jsonld::iri_rs"
//!     }
//! }
//!
//! // `NAME` is an `iri_rs::Iri<&'static str>` resolved at compile time.
//! assert_eq!(vocab::NAME.as_str(), "https://schema.org/name");
//! ```
//!
//! The macro also emits the compact-to-expanded term mapping, letting you go
//! from a term to its IRI without a runtime context lookup. See [`vocab!`]
//! for the full syntax, and note that it requires `iri-rs` with its `static`
//! feature in the calling crate.
//!
//! # Feature flags
//!
//! No feature other than `fast-hash` is enabled by default.
//!
//! ## IRI comparison
//!
//! | Feature | Effect |
//! |---|---|
//! | `fast-hash` *(default)* | Forwarded to `iri-rs`, where it makes `Iri`/`IriRef` compare, hash and order **byte-wise** rather than by RFC 3987 normalization. |
//!
//! This matches how JSON-LD produces IRIs. The [IRI Expansion algorithm][1]
//! resolves against the base IRI using only the basic algorithm of RFC 3986
//! §5.2, and states that "neither Syntax-Based Normalization nor Scheme-Based
//! Normalization are performed" — so IRIs reach the algorithms in whatever form
//! the document wrote them, and comparing them bytewise compares exactly what
//! the specification produced. Byte comparison is also considerably cheaper.
//!
//! Turning the feature off buys normalization-aware equality, where two
//! spellings of the same IRI compare equal, at the cost of normalizing on every
//! comparison. The W3C test suites pass either way. Note that this feature does
//! **not** select the hasher used by this crate's own maps and sets; see below.
//!
//! [1]: https://www.w3.org/TR/json-ld11-api/#iri-expansion
//!
//! ## Hashing
//!
//! The collections used internally are hashed with
//! [`foldhash`](https://docs.rs/foldhash) by default (via `hashbrown`), which
//! is fast but not resistant to hash-flooding from untrusted input. Two
//! features swap that out:
//!
//! | Feature | Effect |
//! |---|---|
//! | `ahash` | Use [`ahash`](https://docs.rs/ahash) as the default hasher. |
//! | `gxhash` | Use [`gxhash`](https://docs.rs/gxhash) as the default hasher. Requires a CPU with AES intrinsics and fails to build without them. |
//!
//! ## JSON interop
//!
//! See [Interop with other JSON crates](#interop-with-other-json-crates).
//!
//! | Feature | Effect |
//! |---|---|
//! | `serde` | Implement [`serde::Serialize`] / `Deserialize` for the syntax and document types. |
//! | `serde-json` | Convert between [`jstrict::Value`] and [`serde_json::Value`]. |
//! | `sonic-rs` | Convert between [`jstrict::Value`] and [`sonic_rs::Value`]. |
//!
//! ## Loading remote documents
//!
//! | Feature | Effect |
//! |---|---|
//! | `reqwest` | Provide `loader::ReqwestLoader`, an HTTP loader for remote contexts and documents. Requires a [`tokio`](https://tokio.rs) runtime. |
//!
//! ## Deriving expansion
//!
//! | Feature | Effect |
//! |---|---|
//! | `expandable` | Provide the [`Expandable`] derive macro and its [`JsonValue`] abstraction. Enables no JSON backend on its own. |
//! | `expandable-serde-json` | `expandable` plus a [`JsonValue`] implementation for [`serde_json::Value`]. |
//! | `expandable-jstrict` | `expandable` plus a [`JsonValue`] implementation for [`jstrict::Value`]. |
//! | `expandable-sonic-rs` | `expandable` plus a [`JsonValue`] implementation for [`sonic_rs::Value`]. Implies `sonic-rs`. |
//! | `expandable-chrono` | `expandable` plus rendering of [`chrono`](https://docs.rs/chrono) date and time types as their XSD lexical forms. |
//!
//! ## Vocabularies
//!
//! | Feature | Effect |
//! |---|---|
//! | `vocab` | Provide the [`vocab!`] macro, which turns JSON-LD context files into compile-time IRI constants. |
//!
//! [`serde::Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html
//!
//! # Fast IRIs and Blank Node Identifiers
//!
//! This library gives you the opportunity to use any datatype you want to
//! represent IRIs an Blank Node Identifiers. Most types have them
//! parameterized.
//! To avoid unnecessary allocations and expensive comparisons, it is highly
//! recommended to use a cheap, lightweight datatype such as
//! [`rdfx::vocabulary::IriIndex`]. This type will represent each distinct
//! IRI/blank node identifier with a unique index. In this case a
//! [`rdfx::vocabulary::IndexVocabulary`] that maps each index back to its
//! original IRI or blank node identifier can be passed to every function.
//!
//! You can also use your own index type, with your own
//! [`rdfx::vocabulary::Vocabulary`] implementation.
//!
//!
//! ## Displaying vocabulary-dependent values
//!
//! Since using vocabularies separates IRIs and Blank ids from their textual
//! representation, it complicates displaying data using them.
//! Fortunately many types defined by this crate implement the
//! [`contextual::DisplayWithContext`] trait that allow displaying value with
//! a "context", which here would be the vocabulary.
//! By importing the [`contextual::WithContext`] which provides the `with`
//! method you can display such value like this:
//! ```
//! use iri_rs::{iri, IriBuf};
//! use rdfx::vocabulary::{IriVocabularyMut, IndexVocabulary};
//! use contextual::WithContext;
//!
//! let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
//! let i = vocabulary.insert(iri!("https://docs.rs/contextual"));
//! let value = jsonld::Id::iri(i);
//!
//! println!("{}", value.with(&vocabulary))
//! ```
//!
//! [`contextual::DisplayWithContext`]: https://docs.rs/contextual/latest/contextual/trait.DisplayWithContext.html
//! [`contextual::WithContext`]: https://docs.rs/contextual/latest/contextual/trait.WithContext.html
// On docs.rs, label every feature-gated item with the feature that unlocks it.
// `doc(auto_cfg)` is still nightly-gated, and `docsrs` is set by docs.rs itself
// (see `rustdoc-args` in Cargo.toml), so stable builds are unaffected.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, doc(auto_cfg))]

pub use jsonld_compaction as compaction;
pub use jsonld_context_processing as context_processing;
pub use jsonld_core::*;
pub use jsonld_expansion as expansion;
pub use jsonld_serialization as ser;
pub use jsonld_syntax as syntax;

pub use compaction::Compact;
pub use context_processing::Process;
pub use expansion::Expand;

#[cfg(feature = "expandable")]
pub use jsonld_expandable::Expandable;
#[cfg(feature = "expandable")]
pub use jsonld_expandable_core::{self as expandable_core, Expandable, ExpandableLanguageMap, ExpandableTypeValue, JsonValue, ToJsonValue};

#[cfg(feature = "vocab")]
pub use jsonld_vocab::generate as vocab;

mod processor;
pub use processor::*;

#[doc(hidden)]
pub use iri_rs;
pub use iri_rs::{InvalidIri, Iri, IriBuf, IriRef, IriRefBuf};

pub use rdfx::{self, BlankId, BlankIdBuf};
