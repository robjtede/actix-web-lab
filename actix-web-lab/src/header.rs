//! Experimental typed headers.

use std::{fmt, str::FromStr};

use actix_http::{error::ParseError, header::HeaderValue};

#[cfg(test)]
pub(crate) use self::header_test_helpers::{assert_parse_eq, assert_parse_fail};
pub use crate::{
    cache_control::{CacheControl, CacheDirective},
    clear_site_data::{ClearSiteData, ClearSiteDataDirective},
    content_length::ContentLength,
    forwarded::Forwarded,
    strict_transport_security::StrictTransportSecurity,
    x_forwarded_prefix::{X_FORWARDED_PREFIX, XForwardedPrefix},
};

/// Parses a group of comma-delimited quoted-string headers.
///
/// Notes that `T`'s [`FromStr`] implementation SHOULD NOT try to strip leading or trailing quotes
/// when parsing (or try to enforce them), since the quoted-string grammar itself enforces them and
/// so this function checks for their existence, and strips them before passing to [`FromStr`].
#[inline]
pub(crate) fn from_comma_delimited_quoted_strings<'a, I, T>(all: I) -> Result<Vec<T>, ParseError>
where
    I: Iterator<Item = &'a HeaderValue> + 'a,
    T: FromStr,
{
    let size_guess = all.size_hint().1.unwrap_or(2);
    let mut result = Vec::with_capacity(size_guess);

    for hdr in all {
        let hdr_str = hdr.to_str().map_err(|_| ParseError::Header)?;

        for part in hdr_str.split(',').filter_map(|x| match x.trim() {
            "" => None,
            y => Some(y),
        }) {
            if let Ok(part) = part
                .strip_prefix('"')
                .and_then(|part| part.strip_suffix('"'))
                // reject headers which are not properly quoted-string formatted
                .ok_or(ParseError::Header)?
                .parse()
            {
                result.push(part);
            }
        }
    }

    Ok(result)
}

/// Formats a list of headers into a comma-delimited quoted-string string.
#[inline]
pub(crate) fn fmt_comma_delimited_quoted_strings<'a, I, T>(
    f: &mut fmt::Formatter<'_>,
    mut parts: I,
) -> fmt::Result
where
    I: Iterator<Item = &'a T> + 'a,
    T: 'a + fmt::Display,
{
    let Some(part) = parts.next() else {
        return Ok(());
    };

    write!(f, "\"{part}\"")?;

    for part in parts {
        write!(f, ", \"{part}\"")?;
    }

    Ok(())
}

#[cfg(test)]
mod header_test_helpers {
    use std::fmt;

    use actix_http::header::Header;
    use actix_web::{HttpRequest, test};

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
