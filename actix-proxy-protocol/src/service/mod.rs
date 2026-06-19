//! Transparent Actix stream wrapper for PROXY protocol headers.

mod acceptor;
mod error;
mod stream;

pub use self::{
    acceptor::{Acceptor, AcceptorService},
    error::{HeaderPolicy, ProxyProtocolError},
    stream::ProxyStream,
};
