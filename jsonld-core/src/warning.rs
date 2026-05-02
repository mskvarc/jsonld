use contextual::{DisplayWithContext, WithContext};

/// Warning handler.
///
/// This trait is implemented by the unit type `()` which ignores warnings.
/// You can use [`Print`] or [`PrintWith`] to print warnings on the standard
/// output or implement your own handler.
pub trait Handler<N, W> {
    /// Handle a warning with the given `vocabulary`.
    fn handle(&mut self, vocabulary: &N, warning: W);
}

impl<N, W> Handler<N, W> for () {
    fn handle(&mut self, _vocabulary: &N, _warning: W) {}
}

impl<N, W, H: Handler<N, W>> Handler<N, W> for &mut H {
    fn handle(&mut self, vocabulary: &N, warning: W) {
        H::handle(*self, vocabulary, warning)
    }
}

/// Prints warnings that can be displayed without vocabulary on the standard
/// output.
pub struct Print;

impl<N, W: std::fmt::Display> Handler<N, W> for Print {
    fn handle(&mut self, _vocabulary: &N, warning: W) {
        eprintln!("{warning}")
    }
}

/// Prints warnings with a given vocabulary on the standard output.
pub struct PrintWith;

impl<N, W: DisplayWithContext<N>> Handler<N, W> for PrintWith {
    fn handle(&mut self, vocabulary: &N, warning: W) {
        eprintln!("{}", warning.with(vocabulary))
    }
}

/// In-memory warning buffer used by parallel sibling tasks.
///
/// Each spawned task accumulates warnings into its own buffer; on join the
/// parent drains them in iteration order so the observable warning order
/// matches the sequential implementation.
pub struct WarningBuf<W>(pub Vec<W>);

impl<W> Default for WarningBuf<W> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<W> WarningBuf<W> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<N, W> Handler<N, W> for WarningBuf<W> {
    fn handle(&mut self, _vocabulary: &N, warning: W) {
        self.0.push(warning)
    }
}
