//! Server-Sent Events (SSE) decoder.

#![forbid(unsafe_code)]
#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod decoder;
mod error;
mod event;
mod message;

pub use self::{error::Error, event::Event, message::Message};

pub(crate) const NEWLINE: u8 = b'\n';
pub(crate) const SSE_DELIMITER: &[u8] = b"\n\n";

#[cfg(test)]
mod tests {
    #[macro_export]
    macro_rules! assert_none {
        ($exp:expr) => {{
            let exp = $exp;
            assert!(exp.is_none(), "Expected None; got: {exp:?}");
        }};
    }
}
