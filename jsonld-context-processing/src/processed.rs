use iri_rs::IriBuf;
use jsonld_core::Context;
use rdfx::BlankIdBuf;
use std::ops;

/// An active context paired with a borrow of the `@context` it was built from.
///
/// Both halves are needed downstream: the active context drives expansion and
/// compaction, while the original, syntactic context is what compaction embeds
/// into its output so the result round-trips.
///
/// Derefs to the active context, so a caller that only needs that can use it
/// directly.
pub struct Processed<'l, T = IriBuf, B = BlankIdBuf> {
    /// The context as it was written.
    pub unprocessed: &'l jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: Context<T, B>,
}

impl<'l, T, B> Processed<'l, T, B> {
    /// Pairs an active context with the `@context` it was built from.
    pub fn new(unprocessed: &'l jsonld_syntax::context::Context, processed: Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the `@context` as it was written, before processing.
    pub fn unprocessed(&self) -> &'l jsonld_syntax::context::Context {
        self.unprocessed
    }

    /// Discards the original `@context` and returns just the active context.
    pub fn into_processed(self) -> Context<T, B> {
        self.processed
    }

    /// Borrows both halves as a [`ProcessedRef`].
    pub fn as_ref(&self) -> ProcessedRef<'l, '_, T, B> {
        ProcessedRef {
            unprocessed: self.unprocessed,
            processed: &self.processed,
        }
    }

    /// Clones the original `@context` so the pair no longer borrows it.
    pub fn into_owned(self) -> ProcessedOwned<T, B> {
        ProcessedOwned {
            unprocessed: self.unprocessed.clone(),
            processed: self.processed,
        }
    }
}

impl<'l, T, B> ops::Deref for Processed<'l, T, B> {
    type Target = Context<T, B>;

    fn deref(&self) -> &Self::Target {
        &self.processed
    }
}

impl<'l, T, B> ops::DerefMut for Processed<'l, T, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.processed
    }
}

/// A borrowed active context paired with a borrow of the `@context` it was built
/// from.
///
/// The form document compaction takes its context in, since it only reads both
/// halves. The two lifetimes are independent: the original `@context` normally
/// outlives the active context borrowed from it.
pub struct ProcessedRef<'l, 'a, T, B> {
    /// The context as it was written.
    pub unprocessed: &'l jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: &'a Context<T, B>,
}

impl<'l, 'a, T, B> ProcessedRef<'l, 'a, T, B> {
    /// Pairs a borrowed active context with the `@context` it was built from.
    pub fn new(unprocessed: &'l jsonld_syntax::context::Context, processed: &'a Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the `@context` as it was written, before processing.
    pub fn unprocessed(&self) -> &'l jsonld_syntax::context::Context {
        self.unprocessed
    }

    /// Returns the active context.
    pub fn processed(&self) -> &'a Context<T, B> {
        self.processed
    }
}

/// An active context paired with an owned copy of the `@context` it was built
/// from.
///
/// Borrows nothing, so it can be stored or returned past the lifetime of the
/// document the `@context` was read from.
pub struct ProcessedOwned<T, B> {
    /// The context as it was written.
    pub unprocessed: jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: Context<T, B>,
}

impl<T, B> ProcessedOwned<T, B> {
    /// Pairs an active context with an owned copy of the `@context` it was built
    /// from.
    pub fn new(unprocessed: jsonld_syntax::context::Context, processed: Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the `@context` as it was written, before processing.
    pub fn unprocessed(&self) -> &jsonld_syntax::context::Context {
        &self.unprocessed
    }

    /// Returns the active context.
    pub fn processed(&self) -> &Context<T, B> {
        &self.processed
    }

    /// Borrows both halves as a [`ProcessedRef`].
    pub fn as_ref(&self) -> ProcessedRef<'_, '_, T, B> {
        ProcessedRef {
            unprocessed: &self.unprocessed,
            processed: &self.processed,
        }
    }
}
