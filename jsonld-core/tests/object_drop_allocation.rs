//! Dropping a node whose properties are all value objects allocates no
//! worklist.
//!
//! Its own test binary, because it replaces the global allocator to count
//! allocations.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use iri_rs::IriBuf;
use jsonld_core::{Id, Indexed, Node, Object, Value};
use rdfx::BlankIdBuf;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

/// The system allocator, counting every allocation.
struct CountingAllocator;

/// Allocations made since counting was last reset.
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn dropping_a_node_whose_properties_are_all_value_objects_allocates_no_worklist() {
    let mut node: Node<IriBuf, BlankIdBuf> = Node::new();
    for name in ["a", "b", "c"] {
        node.insert(
            Id::iri(IriBuf::new(format!("http://example.com/{name}")).unwrap()),
            Indexed::none(Object::Value(Value::Json(jstrict::Value::Boolean(true)))),
        );
    }

    ALLOCATIONS.store(0, Ordering::SeqCst);
    drop(node);

    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
}
