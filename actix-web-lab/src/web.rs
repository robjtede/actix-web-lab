//! Experimental services.
//!
//! Analogous to the `web` module in Actix Web.

pub use crate::group::{Group, GroupMiddleware, GroupService};

/// Creates a group of services with common middleware and no path prefix.
///
/// Pass an array or vector for children of the same type, or a tuple for children
/// of different types. Use [`Group::new()`] to start with an empty group.
///
/// See [`Group`] for supported children, registration, and middleware behavior.
pub fn group(services: impl GroupService) -> Group {
    Group::new().service(services)
}

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
