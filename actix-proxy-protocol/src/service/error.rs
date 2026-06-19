use std::{convert::Infallible, fmt, io};

use proxyproto::ParseError;

/// Controls whether incoming streams must start with a PROXY protocol header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HeaderPolicy {
    /// Reject streams that do not start with a PROXY protocol header.
    #[default]
    Required,

    /// Accept streams without a PROXY protocol header and replay any bytes read during detection.
    Optional,
}

impl HeaderPolicy {
    pub(super) const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

/// PROXY protocol acceptor or stream parsing error.
#[derive(Debug)]
pub enum ProxyProtocolError<SvcErr = Infallible> {
    /// An I/O error occurred while reading the header prelude.
    Io(io::Error),

    /// The stream did not start with a PROXY protocol header.
    MissingHeader,

    /// The stream started with a PROXY protocol header, but it was invalid.
    Parse(ParseError),

    /// Wraps service errors.
    Service(SvcErr),
}

impl<SvcErr> fmt::Display for ProxyProtocolError<SvcErr>
where
    SvcErr: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error while reading PROXY protocol header: {err}"),
            Self::MissingHeader => f.write_str("missing PROXY protocol header"),
            Self::Parse(err) => write!(f, "invalid PROXY protocol header: {err}"),
            Self::Service(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl<SvcErr> std::error::Error for ProxyProtocolError<SvcErr>
where
    SvcErr: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::MissingHeader => None,
            Self::Parse(err) => Some(err),
            Self::Service(err) => Some(err),
        }
    }
}

impl ProxyProtocolError<Infallible> {
    /// Casts the infallible service error type returned from acceptors into caller's type.
    pub fn into_service_error<SvcErr>(self) -> ProxyProtocolError<SvcErr> {
        match self {
            Self::Io(err) => ProxyProtocolError::Io(err),
            Self::MissingHeader => ProxyProtocolError::MissingHeader,
            Self::Parse(err) => ProxyProtocolError::Parse(err),
            Self::Service(err) => match err {},
        }
    }
}

impl<SvcErr> From<io::Error> for ProxyProtocolError<SvcErr> {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl<SvcErr> From<ParseError> for ProxyProtocolError<SvcErr> {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}
