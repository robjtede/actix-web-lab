//! Experimental typed headers.

#[cfg(test)]
pub(crate) use self::header_test_helpers::{assert_parse_eq, assert_parse_fail};
pub use crate::{
    cache_control::{CacheControl, CacheDirective},
    content_length::ContentLength,
    forwarded::Forwarded,
    strict_transport_security::StrictTransportSecurity,
    x_forwarded_prefix::{XForwardedPrefix, X_FORWARDED_PREFIX},
};

#[cfg(test)]
mod header_test_helpers {
    use std::fmt;

    use actix_http::header::Header;
    use actix_web::{test, HttpRequest};

    fn req_from_raw_headers<H: Header, I: IntoIterator<Item = V>, V: AsRef<[u8]>>(
        header_lines: I,
    ) -> HttpRequest {
        header_lines
            .into_iter()
            .fold(test::TestRequest::default(), |req, item| {
                req.append_header((H::name(), item.as_ref().to_vec()))
            })
            .to_http_request()
    }

    #[track_caller]
    pub(crate) fn assert_parse_eq<
        H: Header + fmt::Debug + PartialEq,
        I: IntoIterator<Item = V>,
        V: AsRef<[u8]>,
    >(
        headers: I,
        expect: H,
    ) {
        let req = req_from_raw_headers::<H, _, _>(headers);
        assert_eq!(H::parse(&req).unwrap(), expect);
    }

    #[track_caller]
    pub(crate) fn assert_parse_fail<
        H: Header + fmt::Debug,
        I: IntoIterator<Item = V>,
        V: AsRef<[u8]>,
    >(
        headers: I,
    ) {
        let req = req_from_raw_headers::<H, _, _>(headers);
        H::parse(&req).unwrap_err();
    }
}
