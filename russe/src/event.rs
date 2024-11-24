use bytestring::ByteString;

use crate::message::Message;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Message(Message),
    Comment(ByteString),
    Retry(u64),
}
