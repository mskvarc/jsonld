use iri_rs::IriBuf;
use jsonld_core::Context;
use rdfx::BlankIdBuf;
use std::ops;

/// Processed context that also borrows the original, unprocessed, context.
pub struct Processed<'l, T = IriBuf, B = BlankIdBuf> {
    /// The context as it was written.
    pub unprocessed: &'l jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: Context<T, B>,
}

impl<'l, T, B> Processed<'l, T, B> {
    /// Creates a new `Processed`.
    pub fn new(unprocessed: &'l jsonld_syntax::context::Context, processed: Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the unprocessed of this `Processed`.
    pub fn unprocessed(&self) -> &'l jsonld_syntax::context::Context {
        self.unprocessed
    }

    /// Consumes this `Processed`, returning its processed.
    pub fn into_processed(self) -> Context<T, B> {
        self.processed
    }

    /// Borrows this `Processed`.
    pub fn as_ref(&self) -> ProcessedRef<'l, '_, T, B> {
        ProcessedRef {
            unprocessed: self.unprocessed,
            processed: &self.processed,
        }
    }

    /// Converts this `Processed` into an owned one.
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

/// Reference to a processed context that also borrows the original, unprocessed, context.
pub struct ProcessedRef<'l, 'a, T, B> {
    /// The context as it was written.
    pub unprocessed: &'l jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: &'a Context<T, B>,
}

impl<'l, 'a, T, B> ProcessedRef<'l, 'a, T, B> {
    /// Creates a new `ProcessedRef`.
    pub fn new(unprocessed: &'l jsonld_syntax::context::Context, processed: &'a Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the unprocessed of this `ProcessedRef`.
    pub fn unprocessed(&self) -> &'l jsonld_syntax::context::Context {
        self.unprocessed
    }

    /// Returns the processed of this `ProcessedRef`.
    pub fn processed(&self) -> &'a Context<T, B> {
        self.processed
    }
}

/// Processed context that also owns the original, unprocessed, context.
pub struct ProcessedOwned<T, B> {
    /// The context as it was written.
    pub unprocessed: jsonld_syntax::context::Context,
    /// The active context it processed into.
    pub processed: Context<T, B>,
}

impl<T, B> ProcessedOwned<T, B> {
    /// Creates a new `ProcessedOwned`.
    pub fn new(unprocessed: jsonld_syntax::context::Context, processed: Context<T, B>) -> Self {
        Self { unprocessed, processed }
    }

    /// Returns the unprocessed of this `ProcessedOwned`.
    pub fn unprocessed(&self) -> &jsonld_syntax::context::Context {
        &self.unprocessed
    }

    /// Returns the processed of this `ProcessedOwned`.
    pub fn processed(&self) -> &Context<T, B> {
        &self.processed
    }

    /// Borrows this `ProcessedOwned`.
    pub fn as_ref(&self) -> ProcessedRef<'_, '_, T, B> {
        ProcessedRef {
            unprocessed: &self.unprocessed,
            processed: &self.processed,
        }
    }
}
