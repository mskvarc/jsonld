//! Dropping deeply nested expanded documents never recurses.
//!
//! Every fixture is built with a loop and dropped on a thread whose stack is
//! far too small for a recursive drop of its depth.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use iri_rs::IriBuf;
use jsonld_core::{Id, Indexed, IndexedObject, Node, NodeParts, Object, Value, object::List};
use rdfx::BlankIdBuf;
use std::thread;

/// Nesting depth of every deep fixture.
const DEPTH: usize = 100_000;

/// Stack of the thread each deep document is dropped on.
const SMALL_STACK: usize = 64 * 1024;

type TestNode = Node<IriBuf, BlankIdBuf>;

/// Runs `job` on a thread with a [`SMALL_STACK`] and waits for it.
fn on_small_stack<R: Send + 'static>(job: impl FnOnce() -> R + Send + 'static) -> R {
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(job)
        .expect("the thread starts")
        .join()
        .expect("the thread returns instead of overflowing its stack")
}

fn property() -> Id<IriBuf, BlankIdBuf> {
    Id::iri(IriBuf::new("http://example.com/p".to_owned()).unwrap())
}

fn object(node: TestNode) -> IndexedObject<IriBuf, BlankIdBuf> {
    Indexed::none(Object::node(node))
}

#[test]
fn dropping_a_document_with_100k_nodes_nested_through_a_property_on_a_64_kib_thread_returns() {
    let mut node = TestNode::new();
    for _ in 0..DEPTH {
        let mut parent = TestNode::new();
        parent.insert(property(), object(node));
        node = parent;
    }
    on_small_stack(move || drop(node));
}

#[test]
fn dropping_100k_nested_list_objects_on_a_64_kib_thread_returns() {
    let mut list: IndexedObject<IriBuf, BlankIdBuf> = Indexed::none(Object::List(List::new(Vec::new())));
    for _ in 0..DEPTH {
        list = Indexed::none(Object::List(List::new(vec![list])));
    }
    on_small_stack(move || drop(list));
}

#[test]
fn dropping_100k_nested_graph_objects_on_a_64_kib_thread_returns() {
    let mut node = TestNode::new();
    for _ in 0..DEPTH {
        let mut parent = TestNode::new();
        parent.set_graph_entry(Some(vec![object(node)]));
        node = parent;
    }
    on_small_stack(move || drop(node));
}

#[test]
fn dropping_100k_nested_included_nodes_on_a_64_kib_thread_returns() {
    let mut node = TestNode::new();
    for _ in 0..DEPTH {
        let mut parent = TestNode::new();
        parent.set_included(Some(vec![Indexed::none(node)]));
        node = parent;
    }
    on_small_stack(move || drop(node));
}

#[test]
fn dropping_100k_nested_reverse_property_nodes_on_a_64_kib_thread_returns() {
    let mut node = TestNode::new();
    for _ in 0..DEPTH {
        let mut parent = TestNode::new();
        parent.reverse_properties_or_default().insert(property(), Indexed::none(node));
        node = parent;
    }
    on_small_stack(move || drop(node));
}

#[test]
fn dropping_a_json_literal_value_object_holding_a_100k_deep_array_on_a_64_kib_thread_returns() {
    let mut json = jstrict::Value::Null;
    for _ in 0..DEPTH {
        json = jstrict::Value::Array(vec![json].into());
    }
    let mut node = TestNode::new();
    node.insert(property(), Indexed::none(Object::Value(Value::Json(json))));
    on_small_stack(move || drop(node));
}

#[test]
fn node_into_parts_returns_every_member_unchanged() {
    let id = Id::iri(IriBuf::new("http://example.com/s".to_owned()).unwrap());
    let mut node = TestNode::with_id(id.clone());
    node.types_mut_or_default()
        .push(Id::iri(IriBuf::new("http://example.com/T".to_owned()).unwrap()));
    node.insert(property(), object(TestNode::new()));
    node.set_graph_entry(Some(vec![object(TestNode::new())]));
    node.set_included(Some(vec![Indexed::none(TestNode::new())]));
    node.reverse_properties_or_default().insert(property(), Indexed::none(TestNode::new()));

    let NodeParts {
        id: parts_id,
        types,
        graph,
        included,
        properties,
        reverse_properties,
    } = node.into_parts();

    assert_eq!(parts_id, Some(id));
    assert_eq!(types.map(|types| types.len()), Some(1));
    assert_eq!(graph.map(|graph| graph.len()), Some(1));
    assert_eq!(included.map(|included| included.len()), Some(1));
    assert_eq!(properties.len(), 1);
    assert_eq!(reverse_properties.map(|reverse| reverse.len()), Some(1));
}
