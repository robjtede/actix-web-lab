//! Cache-Control typed header.
//!
//! See [`CacheControl`] docs.

use std::{fmt, str};

use actix_http::{
    HttpMessage,
    error::ParseError,
    header::{
        self, Header, HeaderName, HeaderValue, InvalidHeaderValue, TryIntoHeaderValue,
        fmt_comma_delimited, from_comma_delimited,
    },
};

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
///     HttpResponse,
///     http::header::{CacheControl, CacheDirective},
/// };
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32)]));
/// ```
///
/// ```
/// use actix_web::{
///     HttpResponse,
///     http::header::{CacheControl, CacheDirective},
/// };
///
/// let mut builder = HttpResponse::Ok();
/// builder.insert_header(CacheControl(vec![
///     CacheDirective::NoCache,
///     CacheDirective::Private,
///     CacheDirective::MaxAge(360u32),
///     CacheDirective::Extension("foo".to_owned(), Some("bar".to_owned())),
/// ]));
/// ```
///
/// [RFC 7234 §5.2]: https://datatracker.ietf.org/doc/html/rfc7234#section-5.2
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheControl(pub Vec<CacheDirective>);

impl_more::forward_deref_and_mut!(CacheControl => [CacheDirective]);

impl fmt::Display for CacheControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_comma_delimited(f, &self.0[..])
    }
}

impl TryIntoHeaderValue for CacheControl {
    type Error = InvalidHeaderValue;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        HeaderValue::try_from(self.to_string())
    }
}

impl Header for CacheControl {
    fn name() -> HeaderName {
        header::CACHE_CONTROL
    }

    fn parse<M: HttpMessage>(msg: &M) -> Result<Self, ParseError> {
        let headers = msg.headers().get_all(Self::name());
        from_comma_delimited(headers).and_then(|items| {
            if items.is_empty() {
                Err(ParseError::Header)
            } else {
                Ok(CacheControl(items))
            }
        })
    }
}

/// Directives contained in a [`CacheControl`] header.
///
/// [Read more on MDN.](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#cache_directives)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CacheDirective {
    /// The `max-age=N` directive.
    ///
    /// When used as a request directive, it indicates that the client allows a stored response that
    /// is generated on the origin server within N seconds — where `N` may be any non-negative
    /// integer (including 0). [Read more on MDN.][mdn_req]
    ///
    /// When used as a response directive, it indicates that the response remains fresh until `N`
    /// seconds after the response is generated. [Read more on MDN.][mdn_res]
    ///
    /// [mdn_req]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#max-age_2
    /// [mdn_res]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#max-age
    MaxAge(u32),

    /// The `max-stale=N` request directive.
    ///
    /// This directive indicates that the client allows a stored response that is stale within `N`
    /// seconds. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#max-stale
    MaxStale(u32),

    /// The `min-fresh=N` request directive.
    ///
    /// This directive indicates that the client allows a stored response that is fresh for at least
    /// `N` seconds. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control
    MinFresh(u32),

    /// The `s-maxage=N` response directive.
    ///
    /// This directive also indicates how long the response is fresh for (similar to `max-age`)—but
    /// it is specific to shared caches, and they will ignore `max-age` when it is present. [Read
    /// more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#s-maxage
    SMaxAge(u32),

    /// The `no-cache` directive.
    ///
    /// When used as a request directive, it asks caches to validate the response with the origin
    /// server before reuse. [Read more on MDN.][mdn_req]
    ///
    /// When used as a response directive, it indicates that the response can be stored in caches,
    /// but the response must be validated with the origin server before each reuse, even when the
    /// cache is disconnected from the origin server. [Read more on MDN.][mdn_res]
    ///
    /// [mdn_req]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#no-cache_2
    /// [mdn_res]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#no-cache
    NoCache,

    /// The `no-store` directive.
    ///
    /// When used as a request directive, it allows a client to request that caches refrain from
    /// storing the request and corresponding response — even if the origin server's response could
    /// be stored. [Read more on MDN.][mdn_req]
    ///
    /// When used as a response directive, it indicates that the response can be stored in caches,
    /// but the response must be validated with the origin server before each reuse, even when the
    /// cache is disconnected from the origin server. [Read more on MDN.][mdn_res]
    ///
    /// [mdn_req]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#no-store_2
    /// [mdn_res]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#no-store
    NoStore,

    /// The `no-transform` directive.
    ///
    /// This directive, in both request and response contexts, indicates that any intermediary
    /// (regardless of whether it implements a cache) shouldn't transform the response contents.
    /// [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#no-transform
    NoTransform,

    /// The `only-if-cached` request directive.
    ///
    /// This directive indicates that caches should obtain an already-cached response. If a cache
    /// has stored a response, it's reused. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#only-if-cached
    OnlyIfCached,

    /// The `must-revalidate` response directive.
    ///
    /// This directive indicates that the response can be stored in caches and can be reused while
    /// fresh. If the response becomes stale, it must be validated with the origin server before
    /// reuse. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#must-revalidate
    MustRevalidate,

    /// The `proxy-revalidate` response directive.
    ///
    /// This directive is the equivalent of must-revalidate, but specifically for shared caches
    /// only. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#proxy-revalidate
    ProxyRevalidate,

    /// The `must-understand` response directive.
    ///
    /// This directive indicates that a cache should store the response only if it understands the
    /// requirements for caching based on status code. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#must-understand
    MustUnderstand,

    /// The `private` response directive.
    ///
    /// This directive indicates that the response can be stored only in a private cache (e.g. local
    /// caches in browsers). [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#private
    Private,

    /// The `public` response directive.
    ///
    /// This directive indicates that the response can be stored in a shared cache. Responses for
    /// requests with `Authorization` header fields must not be stored in a shared cache; however,
    /// the `public` directive will cause such responses to be stored in a shared cache. [Read more
    /// on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#public
    Public,

    /// The `immutable` response directive.
    ///
    /// This directive indicates that the response will not be updated while it's fresh. [Read more
    /// on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#immutable
    Immutable,

    /// The `stale-while-revalidate` response directive.
    ///
    /// This directive indicates that the cache could reuse a stale response while it revalidates it
    /// to a cache. [Read more on MDN.][mdn]
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#stale-while-revalidate
    StaleWhileRevalidate,

    /// The `stale-if-error` directive.
    ///
    /// When used as a response directive, it indicates that the cache can reuse a stale response
    /// when an origin server responds with an error (500, 502, 503, or 504). [Read more on MDN.][mdn_res]
    ///
    /// [mdn_res]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control#stale-if-error
    StaleIfError,

    /// Extension directive.
    ///
    /// An unknown directives is collected into this variant with an optional argument value.
    Extension(String, Option<String>),
}

impl fmt::Display for CacheDirective {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CacheDirective::*;

        let dir_str = match self {
            MaxAge(secs) => return write!(f, "max-age={secs}"),
            MaxStale(secs) => return write!(f, "max-stale={secs}"),
            MinFresh(secs) => return write!(f, "min-fresh={secs}"),
            SMaxAge(secs) => return write!(f, "s-maxage={secs}"),

            NoCache => "no-cache",
            NoStore => "no-store",
            NoTransform => "no-transform",
            OnlyIfCached => "only-if-cached",

            MustRevalidate => "must-revalidate",
            ProxyRevalidate => "proxy-revalidate",
            MustUnderstand => "must-understand",
            Private => "private",
            Public => "public",

            Immutable => "immutable",
            StaleWhileRevalidate => "stale-while-revalidate",
            StaleIfError => "stale-if-error",

            Extension(name, None) => name.as_str(),
            Extension(name, Some(arg)) => return write!(f, "{name}={arg}"),
        };

        f.write_str(dir_str)
    }
}

impl str::FromStr for CacheDirective {
    type Err = Option<<u32 as str::FromStr>::Err>;

    fn from_str(dir: &str) -> Result<Self, Self::Err> {
        use CacheDirective::*;

        match dir {
            "" => Err(None),

            "no-cache" => Ok(NoCache),
            "no-store" => Ok(NoStore),
            "no-transform" => Ok(NoTransform),
            "only-if-cached" => Ok(OnlyIfCached),
            "must-revalidate" => Ok(MustRevalidate),
            "public" => Ok(Public),
            "private" => Ok(Private),
            "proxy-revalidate" => Ok(ProxyRevalidate),
            "must-understand" => Ok(MustUnderstand),

            "immutable" => Ok(Immutable),
            "stale-while-revalidate" => Ok(StaleWhileRevalidate),
            "stale-if-error" => Ok(StaleIfError),

            _ => match dir
                .split_once('=')
                .map(|(dir, arg)| (dir, arg.trim_matches('"')))
            {
                // empty argument is not allowed
                Some((_dir, "")) => Err(None),

                Some(("max-age", secs)) => secs.parse().map(MaxAge).map_err(Some),
                Some(("max-stale", secs)) => secs.parse().map(MaxStale).map_err(Some),
                Some(("min-fresh", secs)) => secs.parse().map(MinFresh).map_err(Some),
                Some(("s-maxage", secs)) => secs.parse().map(SMaxAge).map_err(Some),

                // unknown but correctly formatted directive+argument
                Some((left, right)) => Ok(Extension(left.to_owned(), Some(right.to_owned()))),

                // unknown directive
                None => Ok(Extension(dir.to_owned(), None)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deref() {
        let mut cache_ctrl = CacheControl(vec![]);
        let _: &[CacheDirective] = &cache_ctrl;
        let _: &mut [CacheDirective] = &mut cache_ctrl;
    }
}

#[cfg(test)]
crate::test::header_test_module! {
    CacheControl,
    test_parse_and_format {
        header_round_trip_test!(no_headers, [b""; 0], None);
        header_round_trip_test!(empty_header, [b""; 1], None);
        header_round_trip_test!(bad_syntax, [b"foo="], None);

        header_round_trip_test!(
            multiple_headers,
            [&b"no-cache"[..], &b"private"[..]],
            Some(CacheControl(vec![
                CacheDirective::NoCache,
                CacheDirective::Private,
            ]))
        );

        header_round_trip_test!(
            argument,
            [b"max-age=100, private"],
            Some(CacheControl(vec![
                CacheDirective::MaxAge(100),
                CacheDirective::Private,
            ]))
        );

        header_round_trip_test!(
            immutable,
            [b"public, max-age=604800, immutable"],
            Some(CacheControl(vec![
                CacheDirective::Public,
                CacheDirective::MaxAge(604800),
                CacheDirective::Immutable,
            ]))
        );

        header_round_trip_test!(
            stale_if_while,
            [b"must-understand, stale-while-revalidate, stale-if-error"],
            Some(CacheControl(vec![
                CacheDirective::MustUnderstand,
                CacheDirective::StaleWhileRevalidate,
                CacheDirective::StaleIfError,
            ]))
        );

        header_round_trip_test!(
            extension,
            [b"foo, bar=baz"],
            Some(CacheControl(vec![
                CacheDirective::Extension("foo".to_owned(), None),
                CacheDirective::Extension("bar".to_owned(), Some("baz".to_owned())),
            ]))
        );

        #[test]
        fn parse_quote_form() {
            let req = test::TestRequest::default()
                .insert_header((header::CACHE_CONTROL, "max-age=\"200\""))
                .finish();

            assert_eq!(
                Header::parse(&req).ok(),
                Some(CacheControl(vec![CacheDirective::MaxAge(200)]))
            )
        }

        #[test]
        fn trailing_equals_fails() {
            let req = test_request!(GET "/"; "cache-control" => "extension=").to_request();
            CacheControl::parse(&req).unwrap_err();
        }
    }
}
