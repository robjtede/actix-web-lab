#![expect(missing_docs)]

use std::{error::Error as StdError, fmt, io};

use err_report::Report;

#[derive(Debug)]
struct WrappedError<E>(E);

impl<E> fmt::Display for WrappedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Wrapped")
    }
}

impl<E> StdError for WrappedError<E>
where
    E: StdError + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn standalone_error_single_line() {
    assert_eq!(
        "Verbatim",
        Report::new(io::Error::other("Verbatim")).to_string(),
    );

    assert_eq!(
        "Passed non-utf8 string",
        Report::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "Passed non-utf8 string"
        ))
        .to_string(),
    );
}

#[test]
fn wrapped_error_single_line() {
    assert_eq!(
        "Wrapped: Verbatim",
        Report::new(WrappedError(io::Error::other("Verbatim"))).to_string(),
    );

    assert_eq!(
        "Wrapped: Passed non-utf8 string",
        Report::new(WrappedError(io::Error::new(
            io::ErrorKind::InvalidData,
            "Passed non-utf8 string"
        )))
        .to_string(),
    );
}

#[test]
fn standalone_error_pretty() {
    assert_eq!(
        "Verbatim",
        Report::new(io::Error::other("Verbatim"))
            .pretty(true)
            .to_string(),
    );

    assert_eq!(
        "Passed non-utf8 string",
        Report::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "Passed non-utf8 string"
        ))
        .pretty(true)
        .to_string(),
    );
}

#[test]
fn wrapped_error_pretty() {
    assert_eq!(
        indoc::indoc! {"
            Wrapped
            
            Caused by:
                  Verbatim"},
        Report::new(WrappedError(io::Error::other("Verbatim")))
            .pretty(true)
            .to_string(),
    );

    assert_eq!(
        indoc::indoc! {"
            Wrapped

            Caused by:
                  Passed non-utf8 string"},
        Report::new(WrappedError(io::Error::other("Root error")))
            .pretty(true)
            .to_string(),
    );

    assert_eq!(
        indoc::indoc! {"
            Wrapped

            Caused by:
               0: Wrapped
               1: Passed non-utf8 string"},
        Report::new(WrappedError(WrappedError(io::Error::other("Root error"))))
            .pretty(true)
            .to_string(),
    );
}
