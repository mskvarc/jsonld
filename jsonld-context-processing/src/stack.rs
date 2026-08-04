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
    /// Create a new stack frame registering the load of the given context URL.
    fn new(previous: Option<Arc<StackNode<I>>>, url: I) -> StackNode<I> {
        StackNode { previous, url }
    }

    /// Checks if this frame or any parent holds the given URL.
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

/// Context processing stack.
///
/// Contains the list of the loaded contexts to detect loops.
#[derive(Clone)]
pub struct ProcessingStack<I> {
    head: Option<Arc<StackNode<I>>>,
}

impl<I> ProcessingStack<I> {
    /// Creates a new empty processing stack.
    pub fn new() -> Self {
        Self { head: None }
    }

    /// Checks if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Returns the number of URLs in the stack.
    pub fn len(&self) -> usize {
        let mut len = 0;
        let mut node = &self.head;
        while let Some(frame) = node {
            len += 1;
            node = &frame.previous;
        }
        len
    }

    /// Checks if the given URL is already in the stack.
    ///
    /// This is used for loop detection.
    pub fn cycle(&self, url: &I) -> bool
    where
        I: PartialEq,
    {
        match &self.head {
            Some(head) => head.contains(url),
            None => false,
        }
    }

    /// Push a new URL to the stack, unless it is already in the stack.
    ///
    /// Returns `true` if the URL was successfully added or
    /// `false` if a loop has been detected.
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
