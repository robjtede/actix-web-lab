//! Clone of the unstable [`std::error::Report`] type.
//!
//! Backtrace support is omitted due to nightly requirement.
//!
//! Copied on 2025-09-09.

pub use core::error::Error;
// pub use core::error::{Request, request_ref, request_value};

// use std::backtrace::Backtrace;
use std::fmt::{self, Write as _};

/// An error reporter that prints an error and its sources.
///
/// Report also exposes configuration options for formatting the error sources, either entirely on a
/// single line, or in multi-line format with each source on a new line.
///
/// `Report` only requires that the wrapped error implement `Error`. It doesn't require that the
/// wrapped error be `Send`, `Sync`, or `'static`.
///
/// # Examples
///
/// ```rust
/// #![feature(error_reporter)]
/// use std::{
///     error::{Error, Report},
///     fmt,
/// };
///
/// #[derive(Debug)]
/// struct SuperError {
///     source: SuperErrorSideKick,
/// }
///
/// impl fmt::Display for SuperError {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "SuperError is here!")
///     }
/// }
///
/// impl Error for SuperError {
///     fn source(&self) -> Option<&(dyn Error + 'static)> {
///         Some(&self.source)
///     }
/// }
///
/// #[derive(Debug)]
/// struct SuperErrorSideKick;
///
/// impl fmt::Display for SuperErrorSideKick {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "SuperErrorSideKick is here!")
///     }
/// }
///
/// impl Error for SuperErrorSideKick {}
///
/// fn get_super_error() -> Result<(), SuperError> {
///     Err(SuperError {
///         source: SuperErrorSideKick,
///     })
/// }
///
/// fn main() {
///     match get_super_error() {
///         Err(e) => println!("Error: {}", Report::new(e)),
///         _ => println!("No error"),
///     }
/// }
/// ```
///
/// This example produces the following output:
///
/// ```console
/// Error: SuperError is here!: SuperErrorSideKick is here!
/// ```
///
/// ## Output consistency
///
/// Report prints the same output via `Display` and `Debug`, so it works well with
/// [`Result::unwrap`]/[`Result::expect`] which print their `Err` variant via `Debug`:
///
/// ```should_panic
/// #![feature(error_reporter)]
/// use std::error::Report;
/// # use std::error::Error;
/// # use std::fmt;
/// # #[derive(Debug)]
/// # struct SuperError {
/// #     source: SuperErrorSideKick,
/// # }
/// # impl fmt::Display for SuperError {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperError is here!")
/// #     }
/// # }
/// # impl Error for SuperError {
/// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
/// #         Some(&self.source)
/// #     }
/// # }
/// # #[derive(Debug)]
/// # struct SuperErrorSideKick;
/// # impl fmt::Display for SuperErrorSideKick {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperErrorSideKick is here!")
/// #     }
/// # }
/// # impl Error for SuperErrorSideKick {}
/// # fn get_super_error() -> Result<(), SuperError> {
/// #     Err(SuperError { source: SuperErrorSideKick })
/// # }
///
/// get_super_error().map_err(Report::new).unwrap();
/// ```
///
/// This example produces the following output:
///
/// ```console
/// thread 'main' panicked at src/error.rs:34:40:
/// called `Result::unwrap()` on an `Err` value: SuperError is here!: SuperErrorSideKick is here!
/// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
/// ```
///
/// ## Return from `main`
///
/// `Report` also implements `From` for all types that implement [`Error`]; this when combined with
/// the `Debug` output means `Report` is an ideal starting place for formatting errors returned
/// from `main`.
///
/// ```should_panic
/// #![feature(error_reporter)]
/// use std::error::Report;
/// # use std::error::Error;
/// # use std::fmt;
/// # #[derive(Debug)]
/// # struct SuperError {
/// #     source: SuperErrorSideKick,
/// # }
/// # impl fmt::Display for SuperError {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperError is here!")
/// #     }
/// # }
/// # impl Error for SuperError {
/// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
/// #         Some(&self.source)
/// #     }
/// # }
/// # #[derive(Debug)]
/// # struct SuperErrorSideKick;
/// # impl fmt::Display for SuperErrorSideKick {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperErrorSideKick is here!")
/// #     }
/// # }
/// # impl Error for SuperErrorSideKick {}
/// # fn get_super_error() -> Result<(), SuperError> {
/// #     Err(SuperError { source: SuperErrorSideKick })
/// # }
///
/// fn main() -> Result<(), Report<SuperError>> {
///     get_super_error()?;
///     Ok(())
/// }
/// ```
///
/// This example produces the following output:
///
/// ```console
/// Error: SuperError is here!: SuperErrorSideKick is here!
/// ```
///
/// **Note**: `Report`s constructed via `?` and `From` will be configured to use the single line
/// output format. If you want to make sure your `Report`s are pretty printed and include backtrace
/// you will need to manually convert and enable those flags.
///
/// ```should_panic
/// #![feature(error_reporter)]
/// use std::error::Report;
/// # use std::error::Error;
/// # use std::fmt;
/// # #[derive(Debug)]
/// # struct SuperError {
/// #     source: SuperErrorSideKick,
/// # }
/// # impl fmt::Display for SuperError {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperError is here!")
/// #     }
/// # }
/// # impl Error for SuperError {
/// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
/// #         Some(&self.source)
/// #     }
/// # }
/// # #[derive(Debug)]
/// # struct SuperErrorSideKick;
/// # impl fmt::Display for SuperErrorSideKick {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "SuperErrorSideKick is here!")
/// #     }
/// # }
/// # impl Error for SuperErrorSideKick {}
/// # fn get_super_error() -> Result<(), SuperError> {
/// #     Err(SuperError { source: SuperErrorSideKick })
/// # }
///
/// fn main() -> Result<(), Report<SuperError>> {
///     get_super_error()
///         .map_err(Report::from)
///         .map_err(|r| r.pretty(true).show_backtrace(true))?;
///     Ok(())
/// }
/// ```
///
/// This example produces the following output:
///
/// ```console
/// Error: SuperError is here!
///
/// Caused by:
///       SuperErrorSideKick is here!
/// ```
pub struct Report<E = Box<dyn Error>> {
    /// The error being reported.
    error: E,
    // /// Whether a backtrace should be included as part of the report.
    // show_backtrace: bool,
    /// Whether the report should be pretty-printed.
    pretty: bool,
}

impl<E> Report<E>
where
    Report<E>: From<E>,
{
    /// Creates a new `Report` from an input error.
    pub fn new(error: E) -> Report<E> {
        Self::from(error)
    }
}

impl<E> Report<E> {
    /// Enable pretty-printing the report across multiple lines.
    ///
    /// # Examples
    ///
    /// ```rust
    /// #![feature(error_reporter)]
    /// use std::error::Report;
    /// # use std::error::Error;
    /// # use std::fmt;
    /// # #[derive(Debug)]
    /// # struct SuperError {
    /// #     source: SuperErrorSideKick,
    /// # }
    /// # impl fmt::Display for SuperError {
    /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    /// #         write!(f, "SuperError is here!")
    /// #     }
    /// # }
    /// # impl Error for SuperError {
    /// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
    /// #         Some(&self.source)
    /// #     }
    /// # }
    /// # #[derive(Debug)]
    /// # struct SuperErrorSideKick;
    /// # impl fmt::Display for SuperErrorSideKick {
    /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    /// #         write!(f, "SuperErrorSideKick is here!")
    /// #     }
    /// # }
    /// # impl Error for SuperErrorSideKick {}
    ///
    /// let error = SuperError {
    ///     source: SuperErrorSideKick,
    /// };
    /// let report = Report::new(error).pretty(true);
    /// eprintln!("Error: {report:?}");
    /// ```
    ///
    /// This example produces the following output:
    ///
    /// ```console
    /// Error: SuperError is here!
    ///
    /// Caused by:
    ///       SuperErrorSideKick is here!
    /// ```
    ///
    /// When there are multiple source errors the causes will be numbered in order of iteration
    /// starting from the outermost error.
    ///
    /// ```rust
    /// #![feature(error_reporter)]
    /// use std::error::Report;
    /// # use std::error::Error;
    /// # use std::fmt;
    /// # #[derive(Debug)]
    /// # struct SuperError {
    /// #     source: SuperErrorSideKick,
    /// # }
    /// # impl fmt::Display for SuperError {
    /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    /// #         write!(f, "SuperError is here!")
    /// #     }
    /// # }
    /// # impl Error for SuperError {
    /// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
    /// #         Some(&self.source)
    /// #     }
    /// # }
    /// # #[derive(Debug)]
    /// # struct SuperErrorSideKick {
    /// #     source: SuperErrorSideKickSideKick,
    /// # }
    /// # impl fmt::Display for SuperErrorSideKick {
    /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    /// #         write!(f, "SuperErrorSideKick is here!")
    /// #     }
    /// # }
    /// # impl Error for SuperErrorSideKick {
    /// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
    /// #         Some(&self.source)
    /// #     }
    /// # }
    /// # #[derive(Debug)]
    /// # struct SuperErrorSideKickSideKick;
    /// # impl fmt::Display for SuperErrorSideKickSideKick {
    /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    /// #         write!(f, "SuperErrorSideKickSideKick is here!")
    /// #     }
    /// # }
    /// # impl Error for SuperErrorSideKickSideKick { }
    ///
    /// let source = SuperErrorSideKickSideKick;
    /// let source = SuperErrorSideKick { source };
    /// let error = SuperError { source };
    /// let report = Report::new(error).pretty(true);
    /// eprintln!("Error: {report:?}");
    /// ```
    ///
    /// This example produces the following output:
    ///
    /// ```console
    /// Error: SuperError is here!
    ///
    /// Caused by:
    ///    0: SuperErrorSideKick is here!
    ///    1: SuperErrorSideKickSideKick is here!
    /// ```
    pub fn pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    // /// Display backtrace if available when using pretty output format.
    // ///
    // /// # Examples
    // ///
    // /// **Note**: Report will search for the first `Backtrace` it can find starting from the
    // /// outermost error. In this example it will display the backtrace from the second error in the
    // /// sources, `SuperErrorSideKick`.
    // ///
    // /// ```rust
    // /// #![feature(error_reporter)]
    // /// #![feature(error_generic_member_access)]
    // /// # use std::error::Error;
    // /// # use std::fmt;
    // /// use std::error::Request;
    // /// use std::error::Report;
    // /// use std::backtrace::Backtrace;
    // ///
    // /// # #[derive(Debug)]
    // /// # struct SuperError {
    // /// #     source: SuperErrorSideKick,
    // /// # }
    // /// # impl fmt::Display for SuperError {
    // /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // /// #         write!(f, "SuperError is here!")
    // /// #     }
    // /// # }
    // /// # impl Error for SuperError {
    // /// #     fn source(&self) -> Option<&(dyn Error + 'static)> {
    // /// #         Some(&self.source)
    // /// #     }
    // /// # }
    // /// #[derive(Debug)]
    // /// struct SuperErrorSideKick {
    // ///     backtrace: Backtrace,
    // /// }
    // ///
    // /// impl SuperErrorSideKick {
    // ///     fn new() -> SuperErrorSideKick {
    // ///         SuperErrorSideKick { backtrace: Backtrace::force_capture() }
    // ///     }
    // /// }
    // ///
    // /// impl Error for SuperErrorSideKick {
    // ///     fn provide<'a>(&'a self, request: &mut Request<'a>) {
    // ///         request.provide_ref::<Backtrace>(&self.backtrace);
    // ///     }
    // /// }
    // ///
    // /// // The rest of the example is unchanged ...
    // /// # impl fmt::Display for SuperErrorSideKick {
    // /// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // /// #         write!(f, "SuperErrorSideKick is here!")
    // /// #     }
    // /// # }
    // ///
    // /// let source = SuperErrorSideKick::new();
    // /// let error = SuperError { source };
    // /// let report = Report::new(error).pretty(true).show_backtrace(true);
    // /// eprintln!("Error: {report:?}");
    // /// ```
    // ///
    // /// This example produces something similar to the following output:
    // ///
    // /// ```console
    // /// Error: SuperError is here!
    // ///
    // /// Caused by:
    // ///       SuperErrorSideKick is here!
    // ///
    // /// Stack backtrace:
    // ///    0: rust_out::main::_doctest_main_src_error_rs_1158_0::SuperErrorSideKick::new
    // ///    1: rust_out::main::_doctest_main_src_error_rs_1158_0
    // ///    2: rust_out::main
    // ///    3: core::ops::function::FnOnce::call_once
    // ///    4: std::sys::backtrace::__rust_begin_short_backtrace
    // ///    5: std::rt::lang_start::{{closure}}
    // ///    6: std::panicking::try
    // ///    7: std::rt::lang_start_internal
    // ///    8: std::rt::lang_start
    // ///    9: main
    // ///   10: __libc_start_main
    // ///   11: _start
    // /// ```
    // pub fn show_backtrace(mut self, show_backtrace: bool) -> Self {
    //     self.show_backtrace = show_backtrace;
    //     self
    // }
}

impl<E> Report<E>
where
    E: Error,
{
    // fn backtrace(&self) -> Option<&Backtrace> {
    //     // have to grab the backtrace on the first error directly since that error may not be
    //     // 'static
    //     let backtrace = request_ref(&self.error);
    //     let backtrace = backtrace.or_else(|| {
    //         self.error
    //             .source()
    //             .map(|source| Source::new(source).find_map(|source| request_ref(source)))
    //             .flatten()
    //     });
    //     backtrace
    // }

    /// Format the report as a single line.
    fn fmt_singleline(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;

        let sources = self.error.source().into_iter().flat_map(Source::new);

        for cause in sources {
            write!(f, ": {cause}")?;
        }

        Ok(())
    }

    /// Format the report as multiple lines, with each error cause on its own line.
    fn fmt_multiline(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = &self.error;

        write!(f, "{error}")?;

        if let Some(cause) = error.source() {
            write!(f, "\n\nCaused by:")?;

            let multiple = cause.source().is_some();

            for (error, ind) in Source::new(cause).enumerate() {
                writeln!(f)?;
                let mut indented = Indented { inner: f };
                if multiple {
                    write!(indented, "{ind: >4}: {error}")?;
                } else {
                    write!(indented, "      {error}")?;
                }
            }
        }

        // if self.show_backtrace {
        //     if let Some(backtrace) = self.backtrace() {
        //         write!(
        //             f,
        //             "\n\nStack backtrace:\n{}",
        //             backtrace.to_string().trim_end()
        //         )?;
        //     }
        // }

        Ok(())
    }
}

impl<E> From<E> for Report<E>
where
    E: Error,
{
    fn from(error: E) -> Self {
        Report {
            error,
            // show_backtrace: false,
            pretty: false,
        }
    }
}

impl<E> fmt::Display for Report<E>
where
    E: Error,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pretty {
            self.fmt_multiline(f)
        } else {
            self.fmt_singleline(f)
        }
    }
}

// This type intentionally outputs the same format for `Display` and `Debug`for
// situations where you unwrap a `Report` or return it from main.
impl<E> fmt::Debug for Report<E>
where
    Report<E>: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Wrapper type for indenting the inner source.
struct Indented<'a, D> {
    inner: &'a mut D,
}

impl<T> fmt::Write for Indented<'_, T>
where
    T: fmt::Write,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for (i, line) in s.split('\n').enumerate() {
            if i > 0 {
                self.inner.write_char('\n')?;
                self.inner.write_str("      ")?;
            }

            self.inner.write_str(line)?;
        }

        Ok(())
    }
}

/// An iterator over an [`Error`] and its sources.
///
/// If you want to omit the initial error and only process
/// its sources, use `skip(1)`.
#[derive(Clone, Debug)]
struct Source<'a> {
    current: Option<&'a (dyn Error + 'static)>,
}

impl<'a> Source<'a> {
    fn new(error: &'a (dyn Error + 'static)) -> Self {
        Self {
            current: Some(error),
        }
    }
}

impl<'a> Iterator for Source<'a> {
    type Item = &'a (dyn Error + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        self.current = self.current.and_then(Error::source);
        current
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.current.is_some() {
            (1, None)
        } else {
            (0, Some(0))
        }
    }
}
