use jsonld_core::HashMap;
use parking_lot::Mutex;
use std::{hash::Hash, sync::Arc};

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
///
/// Every copy of a stack also shares one record of the contexts dereferenced
/// since the stack was created, which is how one run of the algorithm honours
/// step 5.2.4: "If context was previously dereferenced, then the processor MUST
/// NOT do a further dereference". The chain is per branch of the recursion; the
/// record is per run.
pub struct ProcessingStack<I> {
    head: Option<Arc<StackNode<I>>>,
    dereferenced: Arc<Mutex<HashMap<I, Arc<jsonld_syntax::context::Context>>>>,
}

impl<I> Clone for ProcessingStack<I> {
    // Written by hand because a derive would demand `I: Clone`, while copying
    // the stack only bumps the two `Arc`s it holds.
    fn clone(&self) -> Self {
        Self {
            head: self.head.clone(),
            dereferenced: Arc::clone(&self.dereferenced),
        }
    }
}

impl<I> ProcessingStack<I> {
    /// Creates an empty stack, meaning no remote context is being processed,
    /// with an empty record of dereferenced contexts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: None,
            dereferenced: Arc::new(Mutex::new(HashMap::default())),
        }
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
    /// skips the context only while validating a scoped context (step 5.2.2)
    /// and otherwise processes it again, which is [`Self::enter`].
    pub fn push(&mut self, url: I) -> bool
    where
        I: PartialEq,
    {
        if self.cycle(&url) {
            false
        } else {
            self.enter(url);
            true
        }
    }

    /// Pushes `url` onto the stack whether or not it is already there.
    ///
    /// This is step 5.2.3's "add context to remote contexts": under JSON-LD 1.1
    /// a context named again outside scoped-context validation is processed
    /// again, and each processing counts toward [`MAX_REMOTE_CONTEXTS`].
    pub fn enter(&mut self, url: I) {
        let previous = self.head.take();
        self.head = Some(Arc::new(StackNode::new(previous, url)));
    }

    /// The `@context` of the document dereferenced for `url` earlier in this
    /// run, if there was one (step 5.2.4).
    pub fn dereferenced(&self, url: &I) -> Option<Arc<jsonld_syntax::context::Context>>
    where
        I: Eq + Hash,
    {
        self.dereferenced.lock().get(url).map(Arc::clone)
    }

    /// Records the `@context` dereferenced for `url`, so the rest of this run
    /// reuses it instead of dereferencing `url` again (step 5.2.4).
    pub fn remember_dereferenced(&self, url: I, context: Arc<jsonld_syntax::context::Context>)
    where
        I: Eq + Hash,
    {
        self.dereferenced.lock().insert(url, context);
    }
}

impl<I> Default for ProcessingStack<I> {
    fn default() -> Self {
        Self::new()
    }
}
