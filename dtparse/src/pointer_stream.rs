use crate::{
    errors::Errors,
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        Report, ReportColPointer, ReportFilePointer, ReportLinePointer, ReportTextPointer,
    },
    string::Utf8DecodingError,
};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

pub trait RawPointerStream {
    /// Absolute byte offset of the previously consumed symbol
    ///
    /// # Panics
    /// The underlying stream must panic if nothing was consumed before.
    fn prev_offset(&self) -> usize;
}

pub trait PointerStream {
    /// Position of last consumed byte
    fn prev_ptr(&self) -> Pos;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pos {
    file: Rc<PathBuf>,
    line: usize,
    line_start_byte: usize,
    col: usize,
}

impl Pos {
    pub fn new(file: Rc<PathBuf>, line: usize, line_start_byte: usize, col: usize) -> Self {
        Self {
            file,
            line,
            line_start_byte,
            col,
        }
    }
}

impl ReportFilePointer for Pos {
    fn file(&self) -> &Path {
        &self.file
    }
}

impl ReportLinePointer for Pos {
    fn line(&self) -> &usize {
        &self.line
    }

    fn line_start_byte(&self) -> &usize {
        &self.line_start_byte
    }
}

impl ReportColPointer for Pos {
    fn col(&self) -> &usize {
        &self.col
    }
}

impl ReportTextPointer for Pos {}

#[derive(Debug)]
pub struct RawPointerTracker<'a, I> {
    source: &'a mut I,
    offset: Option<usize>,
}

impl<'a, I> RawPointerTracker<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self {
            source,
            offset: None,
        }
    }
}

impl<'a, I: Iterator<Item = Result<u8, crate::Error>>> Iterator for RawPointerTracker<'a, I> {
    type Item = Result<u8, crate::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = match self.source.next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Some(Err(e)),
            None => return None,
        };
        self.offset = Some(match self.offset {
            Some(v) => v + 1,
            None => 0,
        });
        Some(Ok(b))
    }
}

impl<I> RawPointerStream for RawPointerTracker<'_, I> {
    fn prev_offset(&self) -> usize {
        match self.offset {
            Some(v) => v,
            None => panic!("pointer getter called before consuming any values"),
        }
    }
}

/// Will keep track of the position within the text and will transform the nested (inner) error
/// into a report.
#[derive(Debug)]
pub struct PointerTracker<'a, I> {
    source: &'a mut I,
    file: Rc<PathBuf>,
    line: usize,
    /// byte offset of the line start
    line_offset: usize,
    col: usize,
}

impl<'a, I> PointerTracker<'a, I> {
    pub fn new(source: &'a mut I, file: PathBuf) -> Self {
        Self {
            source,
            file: file.into(),
            line: 0,
            line_offset: 0,
            col: 0,
        }
    }
}

impl<I: RawPointerStream + Iterator<Item = Result<Result<char, Utf8DecodingError>, crate::Error>>>
    Iterator for PointerTracker<'_, I>
{
    type Item = Result<Result<char, Box<dyn Report>>, crate::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let ch = match self.source.next() {
            Some(Ok(Ok(v))) => v,
            Some(Ok(Err(Utf8DecodingError { byte_count: span }))) => {
                return Some(Ok(Err(self.decoding_error_report(span))));
            }
            Some(Err(e)) => return Some(Err(e)),
            None => return None,
        };
        if self.col == 0 {
            self.line_offset = self.source.prev_offset();
        }
        self.col += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 0;
        }
        Some(Ok(Ok(ch)))
    }
}

impl<I: RawPointerStream> PointerTracker<'_, I> {
    fn decoding_error_report(&self, span: usize) -> Box<dyn Report> {
        Box::new(PrimitiveReport::single(PrimitiveReportSegment::single(
            PrimitiveMainMessage::error(
                "invalid UTF-8 character".to_string(),
                Errors::InvalidUtf8Character.id(),
            ),
            PrimitiveReportMessage::error(
                format!(
                    "invalid byte encountered at offset `{}`",
                    self.source.prev_offset(),
                ),
                span,
                self.prev_ptr(),
            ),
        )))
    }
}

impl<I> PointerStream for PointerTracker<'_, I> {
    fn prev_ptr(&self) -> Pos {
        Pos {
            file: self.file.clone(),
            line: self.line,
            line_start_byte: self.line_offset,
            col: self.col,
        }
    }
}

#[cfg(test)]
mod test {
    use super::{RawPointerStream, RawPointerTracker};

    #[test]
    fn raw_offset() {
        let mut src = b"Hello, world!".iter().map(|v| Ok(*v));
        let mut stream = RawPointerTracker::new(&mut src);
        let _ = (&mut stream).take(6).collect::<Vec<_>>();
        assert_eq!(stream.prev_offset(), 5);
    }

    #[test]
    #[should_panic]
    fn no_consumed() {
        let mut src = b"Hello, world!".iter().map(|v| Ok(*v));
        let mut stream = RawPointerTracker::new(&mut src);
        let _ = (&mut stream).take(0).collect::<Vec<_>>();
        stream.prev_offset();
    }
}
