//! Regression tests for silent output-corruption bugs found during the
//! pre-publish review. These paths are not covered by the W3C conformance
//! suite.
#![allow(clippy::unwrap_used)]

use contextual::IntoRefWithContext;
use iri_rs::IriBuf;
use jsonld_core::{
    Container,
    ContainerKind,
    ExpandedDocument,
    Id,
    Indexed,
    Node,
    Object,
    ValidId,
    object::{IndexedEntryKeyRef, List, value},
};
use jsonld_syntax::IntoJsonWithContext;
use rdfx::{BlankIdBuf, vocabulary::no_vocabulary};

/// `["@graph", "@set"]` + `@index` must map to graph+index+set, not
/// graph+id+set.
#[test]
fn graph_set_with_index_maps_to_graph_index_set() {
    assert_eq!(Container::GraphSet.with(ContainerKind::Index), Some(Container::GraphIndexSet));
}

/// The `@included` entry must serialize under the `"@included"` key (it used
/// to come out as `"@include"`, which is not a JSON-LD keyword).
#[test]
fn node_included_serializes_as_included_keyword() {
    let mut node: Node<IriBuf, BlankIdBuf> = Node::new();
    node.set_included(Some(vec![Indexed::none(Node::new())]));

    let json = node.into_json_with(no_vocabulary());
    let object = json.as_object().unwrap();

    assert!(object.get_unique("@included").unwrap().is_some());
    assert!(object.get_unique("@include").unwrap().is_none());
}

/// The `@index` entry key must render as `"@index"` (it used to render as
/// `"@value"`).
#[test]
fn indexed_entry_key_renders_as_index_keyword() {
    let key: IndexedEntryKeyRef<'_, IriBuf, BlankIdBuf> = IndexedEntryKeyRef::Index;
    assert_eq!(key.into_ref_with(no_vocabulary()), "@index");
}

/// `FragmentRef::is_json_object` must report the JSON shape of the fragment,
/// not whether it is an array (copy-paste bug from `is_json_array`).
#[test]
fn value_fragment_json_object_is_json_object() {
    let json = jstrict::Value::Object(jstrict::Object::new());
    let fragment = jsonld_core::object::FragmentRef::<IriBuf, BlankIdBuf>::ValueFragment(value::FragmentRef::JsonFragment(jstrict::FragmentRef::Value(&json)));

    assert!(fragment.is_json_object());
    assert!(!fragment.is_json_array());
}

/// Traversal must descend into `@list` contents; `blank_ids` used to miss
/// every blank node identifier inside a list.
#[test]
fn traverse_descends_into_lists() {
    let blank = BlankIdBuf::new("_:b0".to_string()).unwrap();

    let mut node: Node<IriBuf, BlankIdBuf> = Node::new();
    node.insert(
        Id::Valid(ValidId::Iri(IriBuf::new("http://example.com/p".to_string()).unwrap())),
        Indexed::none(Object::List(List::new(vec![Indexed::none(Object::node(Node::with_id(Id::Valid(
            ValidId::Blank(blank.clone()),
        ))))]))),
    );

    let mut document = ExpandedDocument::new();
    document.insert(Indexed::none(Object::node(node)));

    assert!(document.blank_ids().contains(&blank));
}
