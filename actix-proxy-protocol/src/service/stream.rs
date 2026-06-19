use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use actix_rt::net::{ActixStream, Ready};
use proxyproto::{Header, ParseError, v1, v2};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, ReadBuf};

use super::{HeaderPolicy, ProxyProtocolError};

pin_project_lite::pin_project! {
    /// Stream wrapper that consumes a leading PROXY protocol header and then behaves like `IO`.
    #[derive(Debug)]
    pub struct ProxyStream<IO> {
        #[pin]
        io: IO,
        header: Option<Header>,
        pending: Vec<u8>,
    }
}

impl<IO> ProxyStream<IO> {
    /// Constructs a wrapper from an already parsed header and stream.
    pub fn new(io: IO, header: Option<Header>) -> Self {
        Self {
            io,
            header,
            pending: Vec::new(),
        }
    }

    /// Returns the parsed PROXY protocol header, if one was present.
    pub const fn header(&self) -> Option<&Header> {
        self.header.as_ref()
    }

    /// Removes and returns the parsed PROXY protocol header, if one was present.
    pub fn take_header(&mut self) -> Option<Header> {
        self.header.take()
    }

    /// Returns a shared reference to the wrapped stream.
    pub const fn get_ref(&self) -> &IO {
        &self.io
    }

    /// Returns a mutable reference to the wrapped stream.
    pub fn get_mut(&mut self) -> &mut IO {
        &mut self.io
    }

    /// Consumes the wrapper and returns the wrapped stream and parsed header.
    pub fn into_parts(self) -> (IO, Option<Header>) {
        (self.io, self.header)
    }
}

impl<IO> ProxyStream<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    /// Reads and consumes a required PROXY protocol header from `io`.
    pub async fn accept(io: IO) -> Result<Self, ProxyProtocolError> {
        Self::accept_with_policy(io, HeaderPolicy::Required).await
    }

    /// Reads and consumes an optional PROXY protocol header from `io`.
    pub async fn accept_optional(io: IO) -> Result<Self, ProxyProtocolError> {
        Self::accept_with_policy(io, HeaderPolicy::Optional).await
    }

    /// Reads and consumes a PROXY protocol header according to `policy`.
    pub async fn accept_with_policy(
        mut io: IO,
        policy: HeaderPolicy,
    ) -> Result<Self, ProxyProtocolError> {
        let mut prefix = Vec::with_capacity(v2::SIGNATURE.len());

        loop {
            let Some(byte) = read_byte(&mut io).await? else {
                return if policy.is_required() {
                    Err(ProxyProtocolError::MissingHeader)
                } else {
                    Ok(Self {
                        io,
                        header: None,
                        pending: prefix,
                    })
                };
            };

            prefix.push(byte);

            if prefix == v1::SIGNATURE.as_bytes() {
                let header = read_v1_header(&mut io, prefix).await?;
                return Ok(Self {
                    io,
                    header: Some(Header::V1(header)),
                    pending: Vec::new(),
                });
            }

            if prefix == v2::SIGNATURE {
                let header = read_v2_header(&mut io, prefix).await?;
                return Ok(Self {
                    io,
                    header: Some(Header::V2(header)),
                    pending: Vec::new(),
                });
            }

            let could_be_v1 = v1::SIGNATURE.as_bytes().starts_with(&prefix);
            let could_be_v2 = v2::SIGNATURE.starts_with(&prefix);

            if !could_be_v1 && !could_be_v2 {
                return if policy.is_required() {
                    Err(ProxyProtocolError::MissingHeader)
                } else {
                    Ok(Self {
                        io,
                        header: None,
                        pending: prefix,
                    })
                };
            }
        }
    }
}

impl<IO> AsyncRead for ProxyStream<IO>
where
    IO: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();

        if !this.pending.is_empty() {
            let len = buf.remaining().min(this.pending.len());
            buf.put_slice(&this.pending[..len]);
            this.pending.drain(..len);
            return Poll::Ready(Ok(()));
        }

        this.io.as_mut().poll_read(cx, buf)
    }
}

impl<IO> AsyncWrite for ProxyStream<IO>
where
    IO: AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().io.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().io.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().io.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().io.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
}

impl<IO> ActixStream for ProxyStream<IO>
where
    IO: ActixStream,
{
    fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        if !self.pending.is_empty() {
            return Poll::Ready(Ok(Ready::READABLE));
        }

        self.io.poll_read_ready(cx)
    }

    fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        self.io.poll_write_ready(cx)
    }
}

async fn read_v1_header<IO>(io: &mut IO, mut bytes: Vec<u8>) -> Result<v1::Header, ParseError>
where
    IO: AsyncRead + Unpin,
{
    while !bytes.ends_with(b"\r\n") {
        if bytes.len() == v1::MAX_HEADER_SIZE {
            return Err(ParseError::invalid(
                "PROXY v1 header exceeds maximum length",
            ));
        }

        let Some(byte) = read_byte(io).await? else {
            return Err(ParseError::invalid("stream ended inside PROXY v1 header"));
        };

        bytes.push(byte);
    }

    v1::Header::try_from_bytes(&bytes)
        .map(|(_, header)| header)
        .map_err(|_| ParseError::invalid("invalid PROXY v1 header"))
}

async fn read_v2_header<IO>(io: &mut IO, mut bytes: Vec<u8>) -> Result<v2::Header, ParseError>
where
    IO: AsyncRead + Unpin,
{
    let mut fixed = [0; 4];
    io.read_exact(&mut fixed).await?;
    bytes.extend_from_slice(&fixed);

    let len = u16::from_be_bytes([fixed[2], fixed[3]]) as usize;
    let mut payload = vec![0; len];
    io.read_exact(&mut payload).await?;
    bytes.extend_from_slice(&payload);

    v2::Header::try_from_bytes(&bytes).map(|(_, header)| header)
}

async fn read_byte<IO>(io: &mut IO) -> io::Result<Option<u8>>
where
    IO: AsyncRead + Unpin,
{
    let mut byte = [0];

    match io.read(&mut byte).await? {
        0 => Ok(None),
        1 => Ok(Some(byte[0])),
        _ => unreachable!("read with one-byte buffer returned more than one byte"),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, duplex};

    use super::*;
    use crate::{AddressFamily, Command, TransportProtocol};

    #[actix_rt::test]
    async fn consumes_v1_header_and_preserves_stream_body() {
        let (mut client, server) = duplex(1024);
        let header = v1::Header::new_inet(
            SocketAddr::from(([192, 0, 2, 1], 12345)),
            SocketAddr::from(([198, 51, 100, 2], 443)),
        );

        actix_rt::spawn(async move {
            header.write_to_tokio(&mut client).await.unwrap();
            client.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
            client.shutdown().await.unwrap();
        });

        let mut stream = ProxyStream::accept(server).await.unwrap();

        assert_eq!(
            stream.header().unwrap().source_addr().unwrap(),
            SocketAddr::from(([192, 0, 2, 1], 12345))
        );

        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert_eq!(body, "GET / HTTP/1.1\r\n\r\n");
    }

    #[actix_rt::test]
    async fn consumes_v2_header_and_preserves_stream_body() {
        let (mut client, server) = duplex(1024);
        let mut header = v2::Header::new(
            Command::Proxy,
            TransportProtocol::Stream,
            AddressFamily::Inet,
            SocketAddr::from(([192, 0, 2, 1], 12345)),
            SocketAddr::from(([198, 51, 100, 2], 443)),
        );
        header.add_tlv(0x05, b"abc123");

        actix_rt::spawn(async move {
            header.write_to_tokio(&mut client).await.unwrap();
            client.write_all(b"payload").await.unwrap();
            client.shutdown().await.unwrap();
        });

        let mut stream = ProxyStream::accept(server).await.unwrap();

        let Header::V2(header) = stream.header().unwrap() else {
            panic!("expected v2 header");
        };

        assert_eq!(
            header.tlvs().collect::<Vec<_>>(),
            vec![(0x05, b"abc123".as_slice())]
        );

        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert_eq!(body, "payload");
    }

    #[actix_rt::test]
    async fn optional_mode_replays_bytes_when_no_header_is_present() {
        let (mut client, server) = duplex(1024);

        actix_rt::spawn(async move {
            client.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
            client.shutdown().await.unwrap();
        });

        let mut stream = ProxyStream::accept_optional(server).await.unwrap();

        assert!(stream.header().is_none());

        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert_eq!(body, "GET / HTTP/1.1\r\n\r\n");
    }
}
