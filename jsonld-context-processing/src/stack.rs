use std::sync::Arc;

/// Maximum depth of the remote context chain.
///
/// The [context processing algorithm][1] mandates a processor-defined limit
/// on the number of entries in the `remote contexts` array; exceeding it is a
/// `context overflow` error. This bounds the damage a hostile loader can do
/// by serving an endless chain of distinct context IRIs.
///
/// [1]: <https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm>
pub const MAX_REMOTE_CONTEXTS: usize = 64;

/// Single frame of the context processing stack.
struct StackNode<I> {
    /// Previous frame.
    previous: Option<Arc<StackNode<I>>>,

    /// URL of the last loaded context.
    url: I,
}

impl<I> StackNode<I> {
    /// Creates a stack frame recording the load of `url` on top of `previous`.
    fn new(previous: Option<Arc<StackNode<I>>>, url: I) -> StackNode<I> {
        StackNode { previous, url }
    }

    /// Checks whether this frame or any frame below it holds `url`.
    fn contains(&self, url: &I) -> bool
    where
        I: PartialEq,
    {
        if self.url == *url {
            true
        } else {
            match &self.previous {
                Some(prev) => prev.contains(url),
                None => false,
            }
        }
    }
}

/// The chain of remote context URLs currently being processed.
///
/// This is the specification's `remote contexts` array. It serves two purposes:
/// spotting a context that includes itself, and bounding the length of a remote
/// context chain at [`MAX_REMOTE_CONTEXTS`]. Whether it is empty also tells the
/// algorithm whether the context being processed came from a remote document,
/// which is what decides if its `@base` entry applies.
///
/// Implemented as an immutable singly-linked list behind [`Arc`]s, so the copy
/// each recursive call receives is a pointer bump rather than a clone of the
/// chain.
#[derive(Clone)]
pub struct ProcessingStack<I> {
    head: Option<Arc<StackNode<I>>>,
}

impl<I> ProcessingStack<I> {
    /// Creates an empty stack, meaning no remote context is being processed.
    #[must_use]
    pub fn new() -> Self {
        Self { head: None }
    }

    /// Checks whether no remote context has been entered, i.e. the context being
    /// processed is not itself remote.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Returns the number of remote contexts on the stack.
    ///
    /// Walks the chain, so this is linear in the stack depth.
    #[must_use]
    pub fn len(&self) -> usize {
        let mut len = 0;
        let mut node = &self.head;
        while let Some(frame) = node {
            len += 1;
            node = &frame.previous;
        }
        len
    }

    /// Checks whether `url` is already on the stack, meaning entering it would
    /// close a loop.
    pub fn cycle(&self, url: &I) -> bool
    where
        I: PartialEq,
    {
        match &self.head {
            Some(head) => head.contains(url),
            None => false,
        }
    }

    /// Pushes `url` onto the stack, unless it is already there.
    ///
    /// Returns `true` when the URL was added, and `false` when it was already on
    /// the stack and nothing changed. What `false` means is version-dependent:
    /// JSON-LD 1.0 treats it as a recursive context inclusion error, while 1.1
    /// simply skips reprocessing the context.
    pub fn push(&mut self, url: I) -> bool
    where
        I: PartialEq,
    {
        if self.cycle(&url) {
            false
        } else {
            let mut head = None;
            std::mem::swap(&mut head, &mut self.head);
            self.head = Some(Arc::new(StackNode::new(head, url)));
            true
        }
    }
}

impl<I> Default for ProcessingStack<I> {
    fn default() -> Self {
        Self::new()
    }
}
