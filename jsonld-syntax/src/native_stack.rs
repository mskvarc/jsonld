//! Keeping the recursion of the JSON-LD algorithms off the caller's native
//! stack.
//!
//! Context conversion, the Context Processing algorithm, Expansion and
//! Compaction all recurse once per nesting level of their input. An embedder
//! may run them on a thread whose stack it has already partly used, such as a
//! Tokio worker deep inside a request handler, so the recursion depth alone
//! cannot decide whether the thread's stack suffices.
//!
//! A synchronous recursion point runs its callee through
//! [`on_sufficient_stack`]. An `async` one wraps its callee in [`Recursion`]:
//! boxing a recursive call moves the callee's *state* to the heap but not its
//! *polling*, because a parent polls its child from inside its own `poll`, so
//! the native stack still deepens by one set of poll frames per level. Either
//! way, when less than [`RED_ZONE`] of the current stack is left, the callee
//! runs on a freshly allocated segment of [`SEGMENT`] bytes instead. Whatever
//! bounds a recursion stays what bounds it; this only guarantees the bound is
//! reached on any thread.

use std::{
    future::Future,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};

/// Stack left below which a recursion level moves to a fresh segment.
///
/// The value `jstrict` uses, so one red zone governs every recursion over a
/// document, from parsing and printing its JSON to the JSON-LD algorithms.
pub const RED_ZONE: usize = jstrict::native_stack::RED_ZONE;

/// Size of each fresh stack segment.
pub const SEGMENT: usize = jstrict::native_stack::SEGMENT;

/// Runs `callee`, on a fresh stack segment when the current one is nearly used.
///
/// Delegates to [`jstrict::native_stack::on_sufficient_stack`].
pub fn on_sufficient_stack<R>(callee: impl FnOnce() -> R) -> R {
    jstrict::native_stack::on_sufficient_stack(callee)
}

/// A recursive call of an `async` algorithm.
///
/// The callee lives on the heap, like a `Box::pin` of it would, and every poll
/// of it runs through [`on_sufficient_stack`], so polling a chain of nested
/// calls cannot exhaust the native stack.
pub struct Recursion<F>(Pin<Box<F>>);

impl<F> Recursion<F> {
    /// Wraps the recursive call `callee`.
    pub fn new(callee: F) -> Self {
        Self(Box::pin(callee))
    }
}

impl<F: Future> Future for Recursion<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let callee = &mut self.get_mut().0;
        on_sufficient_stack(|| callee.as_mut().poll(cx))
    }
}
