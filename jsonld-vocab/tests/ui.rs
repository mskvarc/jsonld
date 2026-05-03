#[test]
fn ui() {
    // Trybuild relocates each test file into a wip directory under
    // `<workspace>/target/tests/trybuild/<pkg>/`, so `CARGO_MANIFEST_DIR`
    // for the macro expansion would point at that wip dir. Override it
    // with this crate's actual manifest dir so fixtures resolve correctly.
    // SAFETY: trybuild test runs single-threaded; setting an env var here
    // is safe.
    unsafe {
        std::env::set_var("JSONLD_VOCAB_BASE_DIR", env!("CARGO_MANIFEST_DIR"));
    }

    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/*.rs");
    cases.compile_fail("tests/ui/fail/*.rs");
}
