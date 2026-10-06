//! Dropping nested JSON-LD objects without recursion.
//!
//! The derived drop of an expanded document recurses once per nesting level,
//! and the Expansion algorithm, which runs on sufficient native stack, builds
//! documents of any depth. [`Node`] and [`List`] are the two types that own
//! JSON-LD objects, and every nesting level passes through one of them, so
//! they drop through [`dismantle`]: every descendant node or list is moved
//! onto a heap worklist and stripped there, so no drop ever sees a nested
//! node or list.
use super::{IndexedNode, List, Node, Object};
use crate::IndexedObject;
use std::mem;

/// A descendant waiting to be dropped by [`dismantle`].
pub(crate) enum Pending<T, B> {
    /// An object bound to a property, in a graph or in a list.
    Object(IndexedObject<T, B>),
    /// An included node or a reverse property node.
    Node(IndexedNode<T, B>),
}

/// Drops every descendant of `pending` without recursing.
pub(crate) fn dismantle<T, B>(pending: &mut Vec<Pending<T, B>>) {
    while let Some(descendant) = pending.pop() {
        match descendant {
            Pending::Object(mut object) => match object.inner_mut() {
                Object::Node(node) => node.release_children(pending),
                Object::List(list) => release_list(list, pending),
                Object::Value(_) => {}
            },
            Pending::Node(mut node) => node.inner_mut().release_children(pending),
        }
    }
}

/// Moves the objects of `list` onto `pending`, keeping only those that own
/// JSON-LD objects themselves.
pub(crate) fn release_list<T, B>(list: &mut List<T, B>, pending: &mut Vec<Pending<T, B>>) {
    pending.extend(list.take_entries().into_iter().filter(owns_objects).map(Pending::Object));
}

/// Whether dropping `object` could recurse: a node or a non-empty list.
/// A value object holds no JSON-LD object, and a JSON literal inside one
/// drops flat on its own.
fn owns_objects<T, B>(object: &IndexedObject<T, B>) -> bool {
    match object.inner() {
        Object::Node(_) => true,
        Object::List(list) => !list.is_empty(),
        Object::Value(_) => false,
    }
}

impl<T, B> Node<T, B> {
    /// Moves every node and list object directly under this node onto
    /// `pending`, leaving it without descendants. Value objects are dropped
    /// in place.
    pub(crate) fn release_children(&mut self, pending: &mut Vec<Pending<T, B>>) {
        if let Some(graph) = self.graph.take() {
            pending.extend(graph.into_iter().filter(owns_objects).map(Pending::Object));
        }
        if let Some(included) = self.included.take() {
            pending.extend(included.into_iter().map(Pending::Node));
        }
        if !self.properties.is_empty() {
            pending.extend(
                mem::take(&mut self.properties)
                    .into_iter()
                    .flat_map(|(_, objects)| objects)
                    .filter(owns_objects)
                    .map(Pending::Object),
            );
        }
        if let Some(reverse) = self.reverse_properties.take() {
            pending.extend(reverse.into_iter().flat_map(|(_, nodes)| nodes).map(Pending::Node));
        }
    }
}

impl<T, B> Drop for Node<T, B> {
    /// Drops the node's descendants iteratively: each one is stripped of its
    /// own descendants before it is dropped, so no drop recurses and the
    /// native stack stays flat whatever the nesting depth. A node whose
    /// properties are all value objects allocates nothing.
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.release_children(&mut pending);
        dismantle(&mut pending);
    }
}

impl<T, B> Drop for List<T, B> {
    /// Drops the list's objects iteratively, as [`Node`] drops its own.
    fn drop(&mut self) {
        let mut pending = Vec::new();
        release_list(self, &mut pending);
        dismantle(&mut pending);
    }
}
