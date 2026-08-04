//! Process-wide string interner used by [`crate::context::definition::Key`].
//!
//! Backed by [`lasso::ThreadedRodeo`]. Interned strings live for the lifetime
//! of the program and are **never freed**.
//!
//! # Untrusted input
//!
//! Every term key of every parsed `@context` is interned, so the interner's
//! memory usage grows with the number of *distinct* context keys the process
//! ever parses — a quantity that is attacker-controlled when parsing
//! untrusted documents. A long-running service accepting arbitrary JSON-LD
//! should bound the size of the documents it accepts (for example via
//! [`Options::max_document_size`] on the `reqwest` loader of the main crate,
//! or an equivalent limit at the transport layer) to keep this growth
//! proportional to the traffic it chooses to serve.
//!
//! [`Options::max_document_size`]: https://docs.rs/jsonld/latest/jsonld/loader/reqwest/struct.Options.html

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

/// Interns the given string and returns the canonical `&'static str` directly.
#[inline]
pub fn intern_static(s: &str) -> &'static str {
    let interner = interner();
    let spur = interner.get_or_intern(s);
    interner.resolve(&spur)
}
