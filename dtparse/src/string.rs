use crate::pointer_stream::RawPointerStream;

#[derive(Debug, Clone)]
pub struct Utf8DecodingError {
    /// the span that should be displayed
    pub byte_count: usize,
}

pub struct StringDecoder<'a, I> {
    source: &'a mut I,
}

impl<'a, I> StringDecoder<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self { source }
    }
}

impl<I: Iterator<Item = Result<u8, crate::Error>>> Iterator for StringDecoder<'_, I> {
    type Item = Result<Result<char, Utf8DecodingError>, crate::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        macro_rules! yeet {
            ($from: expr) => {
                match $from {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                }
            };
        }

        let ch = match yeet!(self.source.next()?) {
            // invalid
            0xc0 | 0xc1 | 0xf5..=0xff |
            // continuation at the start of a sequence
            0x80..=0xbf => Err(1),
            // single-byte character
            v @ 0b0..=0x7f => Ok(char::from_u32(v as u32).unwrap()),
            // two-byte character
            b1 @ 0xc0..=0xdf => match self.req_continuation_bytes::<1>() {
                Ok(Ok([b2])) => Ok(char::from_u32(
                    (b1 as u32 & 0x1f) << (8-2) | b2 as u32).unwrap()
                ),
                Err(e) => return Some(Err(e)),
                Ok(Err(read)) => Err(read + 1),
            }
            // three-byte character
            b1 @ 0xe0..=0xef => match self.req_continuation_bytes::<2>() {
                Ok(Ok([b2, b3])) => Ok(char::from_u32(
                    ((b1 as u32 & 0x1f) << ((8-2)*2)) | ((b2 as u32) << (8-2)) | (b3 as u32)
                ).unwrap()),
                Err(e) => return Some(Err(e)),
                Ok(Err(read)) => Err(read + 1),
            }
            // four-byte character
            b1 @ 0xf0..=0xf7 => match self.req_continuation_bytes() {
                Ok(Ok([b2, b3, b4])) => Ok(char::from_u32(
                    ((b1 as u32 & 0x7) << ((8-2)*3)) |
                    ((b2 as u32) << ((8-2)*2)) |
                    ((b3 as u32) << (8-2)) |
                    (b4 as u32)
                ).unwrap()),
                Err(e) => return Some(Err(e)),
                Ok(Err(read)) => Err(read + 1),
            }
        };

        match ch {
            Ok(v) => Some(Ok(Ok(v))),
            Err(span) => Some(Ok(Err(Utf8DecodingError { byte_count: span }))),
        }
    }
}

impl<I: RawPointerStream> RawPointerStream for StringDecoder<'_, I> {
    fn prev_offset(&self) -> usize {
        self.source.prev_offset()
    }
}

impl<I: Iterator<Item = Result<u8, crate::Error>>> StringDecoder<'_, I> {
    /// Return an unmasked valid continuation bytes or an error.
    fn req_continuation_bytes<const COUNT: usize>(
        &mut self,
    ) -> Result<Result<[u8; COUNT], usize>, crate::Error> {
        let mut res = [0; COUNT];
        for i in 0..COUNT {
            let ch = match self.source.next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Err(e),
                None => return Ok(Err(i + 1)),
            };
            res[i] = match ch {
                0x80..=0xbf => ch & 0x3f,
                _ => return Ok(Err(i + 1)),
            };
        }
        Ok(Ok(res))
    }
}

#[cfg(test)]
mod test {
    use super::StringDecoder;

    #[test]
    fn valid_string() {
        let target = "Hello, world";
        let mut chars = target.as_bytes().iter().map(|v| Ok(*v));
        let decoder = StringDecoder::new(&mut chars);
        let res: String = decoder
            .collect::<Result<Result<String, _>, _>>()
            .unwrap()
            .unwrap();
        assert_eq!(res, target);
    }
}
