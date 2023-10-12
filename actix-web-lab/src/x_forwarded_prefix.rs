//! X-Forwarded-Prefix header.
//!
//! See [`XForwardedPrefix`] docs.

use std::future::{ready, Ready};

use actix_http::{
    body::MessageBody,
    error::ParseError,
    header::{Header, HeaderName, HeaderValue, InvalidHeaderValue, TryIntoHeaderValue},
    HttpMessage,
};
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    http::Uri,
    FromRequest,
};
use derive_more::{Deref, DerefMut, Display};
use http::uri::PathAndQuery;

use crate::middleware_from_fn::Next;

/// TODO
#[allow(clippy::declare_interior_mutable_const)]
pub const X_FORWARDED_PREFIX: HeaderName = HeaderName::from_static("x-forwarded-prefix");

/// The `X-Forwarded-Prefix` header, defined in [RFC XXX §X.X].
///
/// The `X-Forwarded-Prefix` header field is used
///
/// Also see
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
/// builder.insert_header(XForwardedPrefix(vec![CacheDirective::MaxAge(86400u32)]));
/// ```
///
/// TODO
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

/// Reconstructs original path using `X-Forwarded-Prefix` header.
///
/// # Examples
///
/// ```
/// # use actix_web::App;
/// use actix_web_lab::middleware::{from_fn, restore_original_path};
///
/// // TODO
///     # ;
/// ```
pub async fn restore_original_path(
    mut req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> actix_web::Result<ServiceResponse<impl MessageBody>> {
    let mut parts = req.head().uri.clone().into_parts();
    let path_and_query = parts
        .path_and_query
        .unwrap_or(PathAndQuery::from_static("/"));

    let prefix = XForwardedPrefix::parse(&req).unwrap();

    let reconstructed = [prefix.as_str(), path_and_query.as_str()].concat();
    parts.path_and_query = Some(PathAndQuery::from_maybe_shared(reconstructed).unwrap());

    let uri = Uri::from_parts(parts).unwrap();
    req.match_info_mut().get_mut().update(&uri);
    req.head_mut().uri = uri;

    next.call(req).await
}

// #[cfg(test)]
// mod middleware_tests {
//     use super::*;

//     #[test]
//     fn noop() {}
// }

#[derive(Debug, Clone, PartialEq, Eq)]
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
