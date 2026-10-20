//! The algorithms' futures are `Send` for any loader, so a caller generic over
//! its loader can run them on a multi-threaded executor.
//!
//! Each test only has to compile: a future that is not `Send` fails the
//! `assert_send` bound at build time.
use jsonld::{
    Expand,
    Loader,
    Process,
    rdfx::vocabulary::no_vocabulary_mut,
    syntax::{Context, Value},
};

/// Requires `future` to be `Send`.
fn assert_send<F: Send>(future: F) -> F {
    future
}

/// Context processing, generic over the loader, is `Send`.
fn process_with_any_loader<L: Loader>(context: &Context, loader: &L) {
    drop(assert_send(context.process(no_vocabulary_mut(), loader, None)));
}

/// Expansion, generic over the loader, is `Send`.
fn expand_with_any_loader<L: Loader>(document: &Value, loader: &L) {
    drop(assert_send(document.expand_with(no_vocabulary_mut(), loader)));
}

#[test]
fn context_processing_with_a_generic_loader_is_send() {
    process_with_any_loader(&Context::null(), &jsonld::NoLoader);
}

#[test]
fn expansion_with_a_generic_loader_is_send() {
    expand_with_any_loader(&Value::Null, &jsonld::NoLoader);
}
