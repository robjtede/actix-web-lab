#![allow(missing_docs)]

#[rustversion::stable(1.70)] // MSRV
#[test]
fn compile_macros() {
    let t = trybuild::TestCases::new();

    t.pass("tests/trybuild/ok-no-body-type.rs");
    t.pass("tests/trybuild/ok-with-body-type.rs");

    t.compile_fail("tests/trybuild/err-invalid-structures.rs");
}
