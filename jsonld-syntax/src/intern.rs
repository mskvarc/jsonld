//! Process-wide string interner used by [`crate::context::definition::Key`].
//!
//! Backed by [`lasso::ThreadedRodeo`]. Interned strings live for the lifetime
//! of the program — acceptable because the universe of context-key strings is
//! bounded by the contexts a process loads.

use lasso::ThreadedRodeo;
use std::sync::OnceLock;

pub use lasso::Spur;

/// Returns the global interner, initializing it on first call.
pub fn interner() -> &'static ThreadedRodeo {
    static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();
    INTERNER.get_or_init(ThreadedRodeo::new)
}

/// Interns the given string and returns its [`Spur`].
#[inline]
pub fn intern(s: &str) -> Spur {
    interner().get_or_intern(s)
}

/// Resolves a [`Spur`] back to its `&'static str`.
#[inline]
pub fn resolve(spur: Spur) -> &'static str {
    interner().resolve(&spur)
}
