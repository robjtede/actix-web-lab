//! Cache-Control typed header.
//!
//! See [`CacheControl`] docs.

use std::{
    convert::Infallible,
    fmt,
    str::{self, FromStr},
};

use actix_http::{
    HttpMessage,
    error::ParseError,
    header::{
        CLEAR_SITE_DATA, Header, HeaderName, HeaderValue, InvalidHeaderValue, TryIntoHeaderValue,
    },
};

use crate::header::{fmt_comma_delimited_quoted_strings, from_comma_delimited_quoted_strings};

/// The `Clear-Site-Data` header, defined in the [W3C Clear-Site-Data spec].
///
/// Contains a list of [directives](ClearSiteDataDirective) for clearing out various types of data
/// from the user agent.
///
/// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data)
///
/// # ABNF
///
/// ```text
/// Clear-Site-Data = 1#( quoted-string )
/// ```
///
/// # Sample Values
///
/// - `"cache"`
/// - `"cache", "cookies"`
/// - `"*"`
///
/// # Examples
///
/// ```
/// use actix_web::HttpResponse;
/// use actix_web_lab::header::{ClearSiteData, ClearSiteDataDirective};
///
/// let mut res = HttpResponse::Ok();
/// res.insert_header(ClearSiteData(vec![ClearSiteDataDirective::All]));
///
/// // shortcut for the all ("*", wildcard) directive
/// let mut res = HttpResponse::Ok();
/// res.insert_header(ClearSiteData::all());
/// ```
///
/// [W3C Clear-Site-Data spec]: https://www.w3.org/TR/clear-site-data
#[derive(Debug, Clone, PartialEq)]
pub struct ClearSiteData(pub Vec<ClearSiteDataDirective>);

impl_more::forward_deref_and_mut!(ClearSiteData => [ClearSiteDataDirective]);

impl ClearSiteData {
    /// Constructs a Clear-Site-Data header containing the wildcard directive indicating that all
    /// data types should be cleared.
    #[doc(alias = "wildcard")]
    pub fn all() -> Self {
        Self(vec![ClearSiteDataDirective::All])
    }
}

impl fmt::Display for ClearSiteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_comma_delimited_quoted_strings(f, self.0.iter())
    }
}

impl TryIntoHeaderValue for ClearSiteData {
    type Error = InvalidHeaderValue;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        HeaderValue::try_from(self.to_string())
    }
}

impl Header for ClearSiteData {
    fn name() -> HeaderName {
        CLEAR_SITE_DATA
    }

    fn parse<M: HttpMessage>(msg: &M) -> Result<Self, ParseError> {
        let headers = msg.headers().get_all(Self::name());

        let items = from_comma_delimited_quoted_strings(headers)?;

        if items.is_empty() {
            return Err(ParseError::Header);
        }

        Ok(ClearSiteData(items))
    }
}

/// Directives contained in a [`ClearSiteData`] header.
///
/// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#directives)
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ClearSiteDataDirective {
    /// Indicates that the server wishes to clear all types of data for the origin of the response.
    ///
    /// If more data types are added in future versions of this header, they will also be covered by
    /// it.
    #[doc(alias = "wildcard")]
    All,

    /// Indicates that the server wishes to remove locally cached data for the origin of the
    /// response URL.
    ///
    /// Depending on the browser, this might also clear out things like pre-rendered pages, script
    /// caches, WebGL shader caches, or address bar suggestions.
    ///
    /// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#cache)
    Cache,

    /// Indicates that the server wishes to remove all client hints (requested via Accept-CH) stored
    /// for the origin of the response URL.
    ///
    /// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#clienthints)
    ClientHints,

    /// Indicates that the server wishes to remove all cookies for the origin of the response URL.
    ///
    /// HTTP authentication credentials are also cleared out. This affects the entire registered
    /// domain, including subdomains.
    ///
    /// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#cookies)
    Cookies,

    /// Indicates that the server wishes to remove all DOM storage for the origin of the response
    /// URL.
    ///
    /// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#storage)
    Storage,

    /// Indicates that the server wishes to reload all browsing contexts for the origin of the
    /// response.
    ///
    /// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Clear-Site-Data#executioncontexts)
    ExecutionContexts,
}

impl ClearSiteDataDirective {
    const fn directive(&self) -> &'static str {
        use ClearSiteDataDirective::*;

        match self {
            All => "*",
            Cache => "cache",
            ClientHints => "clientHints",
            Cookies => "cookies",
            Storage => "storage",
            ExecutionContexts => "executionContexts",
        }
    }
}

impl fmt::Display for ClearSiteDataDirective {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.directive())
    }
}

impl FromStr for ClearSiteDataDirective {
    type Err = Option<Infallible>;

    fn from_str(dir: &str) -> Result<Self, Self::Err> {
        use ClearSiteDataDirective::*;

        match () {
            _ if dir == All.directive() => Ok(All),
            _ if dir == Cache.directive() => Ok(Cache),
            _ if dir == ClientHints.directive() => Ok(ClientHints),
            _ if dir == Cookies.directive() => Ok(Cookies),
            _ if dir == Storage.directive() => Ok(Storage),
            _ if dir == ExecutionContexts.directive() => Ok(ExecutionContexts),

            _ => Err(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deref() {
        let mut cache_ctrl = ClearSiteData(vec![]);
        let _: &[ClearSiteDataDirective] = &cache_ctrl;
        let _: &mut [ClearSiteDataDirective] = &mut cache_ctrl;
    }
}

#[cfg(test)]
crate::test::header_test_module! {
    ClearSiteData,
    tests_parse_and_format {
        header_round_trip_test!(no_headers, [b""; 0], None);
        header_round_trip_test!(empty_header, [b""; 1], None);
        header_round_trip_test!(bad_syntax, [b"foo="], None);
        header_round_trip_test!(bad_syntax_non_quoted, [b"cache"], None);

        header_round_trip_test!(
            wildcard,
            [b"\"*\""],
            Some(ClearSiteData(vec![
                ClearSiteDataDirective::All,
            ]))
        );

        header_round_trip_test!(
            single_header,
            [&b"\"cache\""[..]],
            Some(ClearSiteData(vec![
                ClearSiteDataDirective::Cache,
            ]))
        );

        header_round_trip_test!(
            single_header_multiple_directives,
            [b"\"cache\", \"storage\""],
            Some(ClearSiteData(vec![
                ClearSiteDataDirective::Cache,
                ClearSiteDataDirective::Storage,
            ]))
        );

        header_round_trip_test!(
            multiple_headers,
            [&b"\"cache\""[..], &b"\"cookies\""[..]],
            Some(ClearSiteData(vec![
                ClearSiteDataDirective::Cache,
                ClearSiteDataDirective::Cookies,
            ]))
        );
    }
}
