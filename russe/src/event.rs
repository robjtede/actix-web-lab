use crate::message::Message;
use bytestring::ByteString;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Message(Message),
    Comment(ByteString),
    Retry(u64),
}
