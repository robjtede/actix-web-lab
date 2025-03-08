//! X-Forwarded-Prefix header.
//!
//! See [`XForwardedPrefix`] docs.

use std::future::{Ready, ready};

use actix_http::{
    HttpMessage,
    error::ParseError,
    header::{Header, HeaderName, HeaderValue, InvalidHeaderValue, TryIntoHeaderValue},
};
use actix_web::FromRequest;
use derive_more::Display;
use http::uri::PathAndQuery;

/// Conventional `X-Forwarded-Prefix` header.
///
/// See <https://github.com/dotnet/aspnetcore/issues/23263#issuecomment-776192575>.
#[allow(clippy::declare_interior_mutable_const)]
pub const X_FORWARDED_PREFIX: HeaderName = HeaderName::from_static("x-forwarded-prefix");

/// The conventional `X-Forwarded-Prefix` header.
///
/// The `X-Forwarded-Prefix` header field is used to signal that a prefix was stripped from the path
/// while being proxied.
///
/// # Example Values
///
/// - `/`
/// - `/foo`
///
/// # Examples
///
/// ```
/// use actix_web::HttpResponse;
/// use actix_web_lab::header::XForwardedPrefix;
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(XForwardedPrefix("/bar".parse().unwrap()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub struct XForwardedPrefix(pub PathAndQuery);

impl_more::impl_deref_and_mut!(XForwardedPrefix => PathAndQuery);

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
            .and_then(|hdr| hdr.to_str().ok())
            .map(|hdr| hdr.trim())
            .filter(|hdr| !hdr.is_empty())
            .and_then(|hdr| hdr.parse::<actix_web::http::uri::PathAndQuery>().ok())
            .filter(|path| path.query().is_none())
            .map(XForwardedPrefix)
            .ok_or(ParseError::Header)
    }
}

#[cfg(test)]
mod header_tests {
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

/// Reconstructed path using X-Forwarded-Prefix header.
///
/// ```
/// # use actix_web::{FromRequest as _, test::TestRequest};
/// # actix_web::rt::System::new().block_on(async {
/// use actix_web_lab::extract::ReconstructedPath;
///
/// let req = TestRequest::with_uri("/bar")
///     .insert_header(("x-forwarded-prefix", "/foo"))
///     .to_http_request();
///
/// let path = ReconstructedPath::extract(&req).await.unwrap();
/// assert_eq!(path.to_string(), "/foo/bar");
/// # })
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub struct ReconstructedPath(pub PathAndQuery);

impl FromRequest for ReconstructedPath {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_http::Payload,
    ) -> Self::Future {
        let parts = req.head().uri.clone().into_parts();
        let path_and_query = parts
            .path_and_query
            .unwrap_or(PathAndQuery::from_static("/"));

        let prefix = XForwardedPrefix::parse(req).unwrap();

        let reconstructed = [prefix.as_str(), path_and_query.as_str()].concat();

        ready(Ok(ReconstructedPath(
            PathAndQuery::from_maybe_shared(reconstructed).unwrap(),
        )))
    }
}

#[cfg(test)]
mod extractor_tests {
    use actix_web::test::{self};

    use super::*;

    #[actix_web::test]
    async fn basic() {
        let req = test::TestRequest::with_uri("/bar")
            .insert_header((X_FORWARDED_PREFIX, "/foo"))
            .to_http_request();

        assert_eq!(
            ReconstructedPath::extract(&req).await.unwrap(),
            ReconstructedPath(PathAndQuery::from_static("/foo/bar")),
        );
    }
}
