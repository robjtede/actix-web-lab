//! Strict-Transport-Security header.
//!
//! See [`StrictTransportSecurity`] docs.

use std::{convert::Infallible, time::Duration};

use actix_web::{
    http::header::{
        Header, HeaderName, HeaderValue, TryIntoHeaderValue, STRICT_TRANSPORT_SECURITY,
    },
    HttpMessage,
};

const SECS_IN_YEAR: u64 = 3600 * 24 * 365;

/// Alias for [`StrictTransportSecurity`].
#[doc(hidden)]
#[deprecated(note = "Renamed to `StrictTransportSecurity`.", since = "0.16.2")]
pub type Hsts = StrictTransportSecurity;

/// HTTP Strict Transport Security (HSTS) configuration.
///
/// Care should be taken when setting up HSTS for your site; misconfiguration can potentially leave
/// parts of your site in an unusable state.
///
/// # `Default`
/// The `Default` implementation uses a 5 minute `max-age` and does not include subdomains or
/// preloading. This default is intentionally conservative to prevent accidental misconfiguration
/// causing irrecoverable problems for users.
///
/// Once you have configured and tested the default HSTS config, [`recommended`](Self::recommended)
/// can be used as a secure default for production.
///
/// # References
/// See the [HSTS page on MDN] for more information.
///
/// [HSTS page on MDN]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security
#[derive(Debug, Clone, Copy)]
#[doc(alias = "hsts", alias = "sts")]
pub struct StrictTransportSecurity {
    duration: Duration,

    /// The `includeSubdomains` directive.
    pub include_subdomains: bool,

    /// The `preload` directive.
    pub preload: bool,
}

impl StrictTransportSecurity {
    /// Constructs a new HSTS configuration using the given `duration`.
    ///
    /// Other values take their default.
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            ..Self::default()
        }
    }

    /// Constructs a secure, production-ready HSTS configuration.
    ///
    /// Uses a `max-age` of 2 years and includes subdomains.
    pub fn recommended() -> Self {
        Self {
            duration: Duration::from_secs(2 * SECS_IN_YEAR),
            include_subdomains: true,
            ..Self::default()
        }
    }

    /// Send `includeSubdomains` directive with header.
    pub fn include_subdomains(mut self) -> Self {
        self.include_subdomains = true;
        self
    }

    /// Send `preload` directive with header.
    ///
    /// See <https://hstspreload.org/> for more information.
    pub fn preload(mut self) -> Self {
        self.preload = true;
        self
    }
}

impl Default for StrictTransportSecurity {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(300),
            include_subdomains: false,
            preload: false,
        }
    }
}

impl TryIntoHeaderValue for StrictTransportSecurity {
    type Error = Infallible;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        let secs = self.duration.as_secs();
        let subdomains = self
            .include_subdomains
            .then(|| "; includeSubDomains")
            .unwrap_or("");
        let preload = self.preload.then(|| "; preload").unwrap_or("");

        // eg: max-age=31536000; includeSubDomains; preload
        let sts = format!("max-age={secs}{subdomains}{preload}")
            .parse()
            .unwrap();

        Ok(sts)
    }
}

impl Header for StrictTransportSecurity {
    fn name() -> HeaderName {
        STRICT_TRANSPORT_SECURITY
    }

    fn parse<M: HttpMessage>(_msg: &M) -> Result<Self, actix_http::error::ParseError> {
        unimplemented!("Strict-Transport-Security header cannot yet be parsed");
    }
}

#[cfg(test)]
mod test {
    use actix_web::HttpResponse;

    use super::*;

    #[test]
    fn hsts_as_header() {
        let res = HttpResponse::Ok()
            .insert_header(StrictTransportSecurity::default())
            .finish();
        assert_eq!(
            res.headers()
                .get(StrictTransportSecurity::name())
                .unwrap()
                .to_str()
                .unwrap(),
            "max-age=300"
        );

        let res = HttpResponse::Ok()
            .insert_header(StrictTransportSecurity::default().include_subdomains())
            .finish();
        assert_eq!(
            res.headers()
                .get(StrictTransportSecurity::name())
                .unwrap()
                .to_str()
                .unwrap(),
            "max-age=300; includeSubDomains"
        );

        let res = HttpResponse::Ok()
            .insert_header(StrictTransportSecurity::default().preload())
            .finish();
        assert_eq!(
            res.headers()
                .get(StrictTransportSecurity::name())
                .unwrap()
                .to_str()
                .unwrap(),
            "max-age=300; preload"
        );

        let res = HttpResponse::Ok()
            .insert_header(
                StrictTransportSecurity::default()
                    .include_subdomains()
                    .preload(),
            )
            .finish();
        assert_eq!(
            res.headers()
                .get(StrictTransportSecurity::name())
                .unwrap()
                .to_str()
                .unwrap(),
            "max-age=300; includeSubDomains; preload"
        );
    }

    #[test]
    fn recommended_config() {
        let res = HttpResponse::Ok()
            .insert_header(StrictTransportSecurity::recommended())
            .finish();
        assert_eq!(
            res.headers().get("strict-transport-security").unwrap(),
            "max-age=63072000; includeSubDomains"
        );
    }
}
