use std::borrow::Cow;

use crate::Redirect;

/// Create a relative or absolute redirect.
///
/// See [`Redirect`] docs for usage details.
///
/// ```rust
/// use actix_web::App;
/// use actix_web_lab::web as web_lab;
///
/// let app = App::new()
///     .service(web_lab::redirect("/one", "/two"));
/// ```
pub fn redirect(from: impl Into<Cow<'static, str>>, to: impl Into<Cow<'static, str>>) -> Redirect {
    Redirect::new(from, to)
}
