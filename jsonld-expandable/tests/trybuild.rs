#[test]
fn jsonld_expandable_trybuild() {
    let t = trybuild::TestCases::new();
    t.pass("tests/expand/pass_*.rs");
    t.compile_fail("tests/expand/fail_*.rs");
}
