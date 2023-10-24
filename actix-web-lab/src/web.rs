//! Experimental services.
//!
//! Analogous to the `web` module in Actix Web.

#[cfg(feature = "spa")]
pub use crate::spa::Spa;

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
