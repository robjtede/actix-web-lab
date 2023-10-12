//! X-Forwarded-Prefix header.
//!
//! See [`XForwardedPrefix`] docs.

use actix_http::{
    error::ParseError,
    header::{Header, HeaderName, HeaderValue, InvalidHeaderValue, TryIntoHeaderValue},
    HttpMessage,
};
use derive_more::{Deref, DerefMut, Display};
use http::uri::PathAndQuery;

/// TODO
#[allow(clippy::declare_interior_mutable_const)]
pub const X_FORWARDED_PREFIX: HeaderName = HeaderName::from_static("x-forwarded-prefix");

/// The `Cache-Control` header, defined in [RFC 7234 §5.2].
///
/// Includes built-in support for directives introduced in subsequent specifications. [Read more
/// about the full list of supported directives on MDN][mdn].
///
/// The `Cache-Control` header field is used to specify [directives](CacheDirective) for caches
/// along the request/response chain. Such cache directives are unidirectional in that the presence
/// of a directive in a request does not imply that the same directive is to be given in the
/// response.
///
/// # ABNF
/// ```text
/// Cache-Control   = 1#cache-directive
/// cache-directive = token [ \"=\" ( token / quoted-string ) ]
/// ```
///
/// # Example Values
/// - `max-age=30`
/// - `no-cache, no-store`
/// - `public, max-age=604800, immutable`
/// - `private, community=\"UCI\"`
///
/// # Examples
/// ```
/// use actix_web::{
///     http::header::{CacheDirective, XForwardedPrefix},
///     HttpResponse,
/// };
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(XForwardedPrefix(vec![CacheDirective::MaxAge(86400u32)]));
/// ```
///
/// ```
/// use actix_web::{
///     http::header::{CacheDirective, XForwardedPrefix},
///     HttpResponse,
/// };
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(XForwardedPrefix(vec![
///     CacheDirective::NoCache,
///     CacheDirective::Private,
///     CacheDirective::MaxAge(360u32),
///     CacheDirective::Extension("foo".to_owned(), Some("bar".to_owned())),
/// ]));
/// ```
///
/// [RFC 7234 §5.2]: https://datatracker.ietf.org/doc/html/rfc7234#section-5.2
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control
#[derive(Debug, Clone, PartialEq, Eq, Deref, DerefMut, Display)]
pub struct XForwardedPrefix(pub PathAndQuery);

impl TryIntoHeaderValue for XForwardedPrefix {
    type Error = InvalidHeaderValue;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        HeaderValue::try_from(self.to_string())
    }
}

impl Header for XForwardedPrefix {
    fn name() -> HeaderName {
        X_FORWARDED_PREFIX
    }

    fn parse<M: HttpMessage>(msg: &M) -> Result<Self, ParseError> {
        let header = msg.headers().get(Self::name());

        header
            .and_then(|hdr| dbg!(hdr).to_str().ok())
            .map(|hdr| dbg!(hdr).trim())
            .filter(|hdr| !dbg!(hdr).is_empty())
            .and_then(|hdr| dbg!(hdr).parse::<actix_web::http::uri::PathAndQuery>().ok())
            .filter(|path| dbg!(path).query().is_none())
            .map(XForwardedPrefix)
            .ok_or(ParseError::Header)
    }
}

#[cfg(test)]
mod tests {
    use actix_web::test::{self};

    use super::*;

    #[test]
    fn deref() {
        let mut fwd_prefix = XForwardedPrefix(PathAndQuery::from_static("/"));
        let _: &PathAndQuery = &fwd_prefix;
        let _: &mut PathAndQuery = &mut fwd_prefix;
    }

    #[test]
    fn no_headers() {
        let req = test::TestRequest::default().to_http_request();
        assert_eq!(XForwardedPrefix::parse(&req).ok(), None);
    }

    #[test]
    fn empty_header() {
        let req = test::TestRequest::default()
            .insert_header((X_FORWARDED_PREFIX, ""))
            .to_http_request();

        assert_eq!(XForwardedPrefix::parse(&req).ok(), None);
    }

    #[test]
    fn single_header() {
        let req = test::TestRequest::default()
            .insert_header((X_FORWARDED_PREFIX, "/foo"))
            .to_http_request();

        assert_eq!(
            XForwardedPrefix::parse(&req).ok().unwrap(),
            XForwardedPrefix(PathAndQuery::from_static("/foo")),
        );
    }

    #[test]
    fn multiple_headers() {
        let req = test::TestRequest::default()
            .append_header((X_FORWARDED_PREFIX, "/foo"))
            .append_header((X_FORWARDED_PREFIX, "/bar"))
            .to_http_request();

        assert_eq!(
            XForwardedPrefix::parse(&req).ok().unwrap(),
            XForwardedPrefix(PathAndQuery::from_static("/foo")),
        );
    }
}
