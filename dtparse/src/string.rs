struct StringDecoder<'a, I> {
    byte: usize,
    line: usize,
    line_start_byte: usize,
    source: &'a mut I,
}

impl<'a, I> StringDecoder<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self { byte: 0, line: 1, line_start_byte: 0, source }
    }
}

impl<I: Iterator<Item = Result<u8, crate::Error>>> Iterator for StringDecoder<'_, I> {
    type Item = Result<Result<char, Box<dyn crate::report::Report>>, crate::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        macro_rules! yeet {
            ($from: expr) => {
                match $from {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                }
            };

            (req $from: expr) => {
                match $from {
                    Some(Ok(v)) => v,
                    _ => return Some(Ok(todo!())),
                }
            }
        }

        match yeet!(self.source.next()?) {
            // invalid
            0xc0 | 0xc1 | 0xf5..=0xff |
            // continuation at the start of a sequence
            0x80..=0xbf => todo!(),
            // single-byte character
            v @ 0x0..=0x7f => v as u32,
            // two-byte character
            b1 @ 0xc0..=0xdf => match yeet!(req self.source.next()) {
                //b2 @ 0x80..=0xbf => (b1 as u32) << 
            }
        }
    }
}
