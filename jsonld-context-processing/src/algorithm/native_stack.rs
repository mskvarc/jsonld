//! Keeping the recursion of the context processing algorithm off the caller's
//! native stack.
//!
//! The algorithm recurses for every remote `@context` it enters (step 5.2.6),
//! for every scoped `@context` it validates (Create Term Definition step 21.3)
//! and for every term a definition depends on. The `async` implementation
//! boxes each recursive call, which moves the callee's *state* to the heap but
//! not its *polling*: a parent polls its child from inside its own `poll`, so
//! the native stack still deepens by one set of poll frames per level. A remote
//! context cycle, which JSON-LD 1.1 reprocesses at the top level until step
//! 5.2.3's context overflow ends it, therefore walks
//! [`MAX_REMOTE_CONTEXTS`](crate::MAX_REMOTE_CONTEXTS) levels deep, and on a
//! thread whose stack the embedder has already partly used, that aborted the
//! process on a stack overflow before the spec-defined error was reached.
//!
//! Every recursion point now runs its callee through [`on_sufficient_stack`]:
//! when less than [`RED_ZONE`] of the current stack is left, the callee runs on
//! a freshly allocated segment of [`SEGMENT`] bytes instead. The depth limits
//! stay what bounds the recursion; this only guarantees they are reached as
//! errors on any thread.

use std::{
    future::Future,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};

/// Stack left below which a recursion level moves to a fresh segment.
///
/// It has to exceed the native stack one level uses between two recursion
/// points, which is largest in unoptimized builds.
const RED_ZONE: usize = 128 * 1024;

/// Size of each fresh stack segment.
const SEGMENT: usize = 2 * 1024 * 1024;

/// Runs `callee`, on a fresh stack segment when the current one is nearly used.
pub(crate) fn on_sufficient_stack<R>(callee: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(RED_ZONE, SEGMENT, callee)
}

/// A recursive call of the `async` algorithm.
///
/// The callee lives on the heap, like the `Box::pin` it replaces, and every
/// poll of it runs through [`on_sufficient_stack`], so polling a chain of
/// nested calls cannot exhaust the native stack.
pub(crate) struct Recursion<F>(Pin<Box<F>>);

impl<F> Recursion<F> {
    /// Wraps the recursive call `callee`.
    pub(crate) fn new(callee: F) -> Self {
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
