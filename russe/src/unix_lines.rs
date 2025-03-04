use std::io;

use bytes::Bytes;

/// A lines iterator that only splits on `\n`.
#[derive(Debug)]
pub(crate) struct UnixLines<R> {
    pub(crate) rdr: R,
}

impl<R: io::BufRead> Iterator for UnixLines<R> {
    type Item = io::Result<Bytes>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = Vec::new();

        match self.rdr.read_until(b'\n', &mut buf) {
            Ok(0) => None,
            Ok(_n) => {
                if buf.ends_with(b"\n") {
                    buf.pop();
                }

                Some(Ok(Bytes::from(buf)))
            }
            Err(err) => Some(Err(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let buf = io::Cursor::new(&b"12\r"[..]);
        let mut lines = UnixLines { rdr: buf };
        assert_eq!(lines.next().unwrap().unwrap(), "12\r");
        assert!(lines.next().is_none());

        let buf = io::Cursor::new(&b"12\r\n\n"[..]);
        let mut lines = UnixLines { rdr: buf };
        assert_eq!(lines.next().unwrap().unwrap(), "12\r");
        assert_eq!(lines.next().unwrap().unwrap(), "");
        assert!(lines.next().is_none());
    }
}
