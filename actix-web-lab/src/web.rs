//! Experimental services.
//!
//! Analogous to the `web` module in Actix Web.

use std::borrow::Cow;

pub use crate::{redirect::Redirect, spa::Spa};

/// Create a relative or absolute redirect.
///
/// See [`Redirect`] docs for usage details.
///
/// # Examples
/// ```
/// use actix_web::App;
/// use actix_web_lab::web as web_lab;
///
/// let app = App::new()
///     .service(web_lab::redirect("/one", "/two"));
/// ```
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
///             .finish()
///     );
/// ```
pub fn spa() -> Spa {
    Spa::default()
}
