//! Experimental services.
//!
//! Analogous to the `web` module in Actix Web.

use std::borrow::Cow;

#[allow(deprecated)]
pub use crate::redirect::Redirect;
#[cfg(feature = "spa")]
pub use crate::spa::Spa;

/// Create a relative or absolute redirect.
///
/// _This feature has [graduated to Actix Web][graduated]. Further development will occur there._
///
/// See [`Redirect`] docs for usage details.
///
/// # Examples
/// ```
/// use actix_web::App;
/// use actix_web_lab::web as web_lab;
///
/// let app = App::new().service(web_lab::redirect("/one", "/two"));
/// ```
///
/// [graduated]: https://docs.rs/actix-web/4/actix_web/web/struct.Redirect.html
#[allow(deprecated)]
#[deprecated(since = "0.19.0", note = "Type has graduated to Actix Web.")]
pub fn redirect(from: impl Into<Cow<'static, str>>, to: impl Into<Cow<'static, str>>) -> Redirect {
    Redirect::new(from, to)
}

/// Constructs a new Single-page Application (SPA) builder.
///
/// See [`Spa`] docs for more details.
///
/// # Examples
/// ```
/// # use actix_web::App;
/// # use actix_web_lab::web::spa;
/// let app = App::new()
///     // ...api routes...
///     .service(
///         spa()
///             .index_file("./examples/assets/spa.html")
///             .static_resources_mount("/static")
///             .static_resources_location("./examples/assets")
///             .finish(),
///     );
/// ```
#[cfg(feature = "spa")]
pub fn spa() -> Spa {
    Spa::default()
}
