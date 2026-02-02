use crate::{
    DisplayWriter,
    file::FileReader,
    pointer_stream::Pos,
    result::IoError,
    styling::{Color, Style},
};

/// A report for an error, a warning, or a note
pub trait Report: std::fmt::Debug {
    fn segments(&self) -> &[Box<dyn ReportSegment>];
}

/// A single segment for the report with one main message
///
/// This can contain multiple inline messages (for different files or lines)
pub trait ReportSegment: std::fmt::Debug {
    /// The main message for this segment
    /// This should be displayed at the start of the segment display
    fn main_message(&self) -> &Option<Box<dyn ReportSegmentMesssage>>;
    /// The messages and pointers to specific segments of text
    ///
    /// This should be sorted where it makes sense - first on the file level, then on the line
    /// level and then on the start column level
    ///
    /// If not sorted - the display will be disjoint with possibly redundant file names
    fn inline_messages(&self) -> &[Box<dyn ReportInlineMessage>];
}

/// The main message of a report segment
pub trait ReportSegmentMesssage: std::fmt::Debug {
    fn display_type(&self) -> &String;
    fn message(&self) -> &String;
    fn color(&self) -> &Color;
    /// Error/warning/info identifier (if any)
    fn id(&self) -> &Option<String>;
}

/// A pointer to a file (based on path)
pub trait ReportFilePointer {
    fn file(&self) -> &std::path::Path;
}

/// A pointer to a specific line in a file
pub trait ReportLinePointer {
    /// The line number, 1-based indexing
    fn line(&self) -> &usize;
    /// The index of the first byte of the pointed-to line (used for fast look-ups)
    fn line_start_byte(&self) -> &usize;
}

/// A pointer to a specific column in a line
pub trait ReportColPointer {
    /// The column number, 1-based indexing (0 is before the first character of the line)
    ///
    /// Non-UTF8 bytes are counted as a single character each
    fn col(&self) -> &usize;
}

/// A pointer to an exact location in a file
pub trait ReportTextPointer: ReportFilePointer + ReportLinePointer + ReportColPointer {}

/// Message with exact location, length and style
pub trait ReportInlineMessage: std::fmt::Debug {
    fn ptr(&self) -> &dyn ReportTextPointer;
    /// The length of the underlined text in columns/characters/bytes
    /// This should not exceed the line - or it will overflow the line length when displaying.
    ///
    /// Overlaps are skipped.
    fn span(&self) -> &usize;
    fn message(&self) -> &str;
    fn underline_symbol(&self) -> &char;
    fn color(&self) -> &Color;
}

#[cfg(test)]
#[derive(Default)]
pub struct BufWriter {
    buf: String,
}

#[cfg(test)]
impl DisplayWriter for BufWriter {
    fn write(&mut self, to_write: impl AsRef<str>) -> Result<usize, IoError> {
        self.buf.push_str(to_write.as_ref());
        Ok(to_write.as_ref().len())
    }

    fn write_rep(&mut self, to_write: char, repeat: usize) -> Result<usize, IoError> {
        for _ in 0..repeat {
            self.buf.push(to_write);
        }
        Ok(repeat)
    }
}

pub struct ReportDisplay<'a> {
    report: &'a dyn Report,
}

impl<'a> ReportDisplay<'a> {
    pub fn new(report: &'a dyn Report) -> Self {
        Self { report }
    }
}

impl ReportDisplay<'_> {
    pub fn write(
        &self,
        file_reader: &mut impl FileReader,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        let segments = self.report.segments();
        let mut written = 0;
        let mut first = true;
        for segment in segments.iter() {
            if !first {
                written += writer.write("\n")?;
            }
            first = false;
            written += SegmentDisplay::new(segment.as_ref()).write(file_reader, writer)?;
        }
        Ok(written)
    }
}

struct SegmentDisplay<'a> {
    segment: &'a dyn ReportSegment,
}

impl<'a> SegmentDisplay<'a> {
    fn new(segment: &'a dyn ReportSegment) -> Self {
        Self { segment }
    }
}

impl SegmentDisplay<'_> {
    fn write(
        &self,
        file_reader: &mut impl FileReader,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        let mut written = 0;
        if let Some(main_message) = self.segment.main_message() {
            written += Style::from(main_message.color()).bold().write(writer)?
                + writer.write(main_message.display_type())?
                + if let Some(id) = main_message.id() {
                    writer.write("[")? + writer.write(id)? + writer.write("]")?
                } else {
                    0
                }
                + Style::default().reset().write(writer)?
                + Style::default().bold().write(writer)?
                + writer.write(": ")?
                + writer.write(main_message.message())?
                + Style::default().reset().write(writer)?
                + writer.write("\n")?;
        }
        Ok(written + self.write_all(file_reader, writer)?)
    }

    fn write_all(
        &self,
        file_reader: &mut impl FileReader,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        let source = self.segment.inline_messages();
        struct Prev<'a> {
            file: std::path::PathBuf,
            line: &'a dyn ReportTextPointer,
            max_lineno_length: usize,
            first_ptr: usize,
        }
        let mut written = 0;
        let mut prev = None;
        for (i, message) in source.iter().enumerate() {
            let (file, line) = (message.ptr().file(), message.ptr().line());
            let Some(ref mut prev) = prev else {
                prev = Some(Prev {
                    file: file.to_path_buf(),
                    line: message.ptr(),
                    max_lineno_length: Self::lineno_length(line),
                    first_ptr: i,
                });
                continue;
            };

            // write disjoint segment
            if prev.file != file || prev.line.line() != line {
                written += Self::write_filepath(
                    &prev.file,
                    *prev.line.line(),
                    *prev.line.col(),
                    prev.max_lineno_length,
                    writer,
                )? + Self::write_line_prefix(None, prev.max_lineno_length, writer)?
                    + writer.write("\n")?
                    + Self::write_line(
                        prev.line,
                        &source[prev.first_ptr..i],
                        prev.max_lineno_length,
                        file_reader,
                        writer,
                    )?;
                *prev = Prev {
                    file: file.to_path_buf(),
                    line: message.ptr(),
                    max_lineno_length: Self::lineno_length(line),
                    first_ptr: i,
                };
                continue;
            }

            // update max line number length
            prev.max_lineno_length =
                std::cmp::max(prev.max_lineno_length, Self::lineno_length(line));
        }

        // write remaining
        if let Some(prev) = prev {
            written += Self::write_filepath(
                &prev.file,
                *prev.line.line(),
                *prev.line.col(),
                prev.max_lineno_length,
                writer,
            )? + Self::write_line_prefix(None, prev.max_lineno_length, writer)?
                + writer.write("\n")?
                + Self::write_line(
                    prev.line,
                    &source[prev.first_ptr..],
                    prev.max_lineno_length,
                    file_reader,
                    writer,
                )?;
        }

        Ok(written)
    }

    fn lineno_length(number: &usize) -> usize {
        if *number == 0 {
            return 1;
        }
        let mut len = 0;
        let mut number = *number;
        while number > 0 {
            len += 1;
            number /= 10;
        }
        len
    }

    fn write_filepath(
        filepath: &std::path::Path,
        line: usize,
        col: usize,
        line_number_length: usize,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        Ok(Self::prefix_style().write(writer)?
            + writer.write_rep(' ', line_number_length)?
            + writer.write("--> ")?
            + Style::default().reset().write(writer)?
            + writer.write(filepath.to_str().ok_or(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "could not convert filepath to string",
            ))?)?
            + writer.write(":")?
            + writer.write(line.to_string())?
            + writer.write(":")?
            + writer.write(col.to_string())?
            + writer.write("\n")?)
    }

    /// # Caution
    /// This assumes that all of the messages are on the same line and sorted by start column
    /// and will not check that they, indeed, are.
    fn write_line(
        line: &dyn ReportTextPointer,
        messages: &[Box<dyn ReportInlineMessage>],
        max_lineno_length: usize,
        file_reader: &mut impl FileReader,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        let mut written = 0;

        // raw line and line number
        let (raw_line, line_length) = file_reader.read_line_lossy(line)?;
        written += Self::write_line_prefix(Some(*line.line()), max_lineno_length, writer)?
            + writer.write(&raw_line)?
            + writer.write("\n")?
            + Self::write_line_prefix(None, max_lineno_length, writer)?;

        // message underlines
        let mut ptr = 1;
        for message in messages.iter() {
            let mut end_ptr = message.ptr().col() + message.span();
            let should_break = if end_ptr > line_length + 1 {
                end_ptr = line_length + 1;
                true
            } else {
                false
            };
            let span = end_ptr.saturating_sub(std::cmp::max(ptr, *message.ptr().col()));
            written += writer.write_rep(' ', message.ptr().col().saturating_sub(ptr))?
                + Style::from(message.color()).bold().write(writer)?
                + writer.write_rep(*message.underline_symbol(), span)?
                + Style::default().reset().write(writer)?;
            ptr = end_ptr;
            if should_break {
                println!("breaking on {line_length}");
                break;
            }
        }
        written += writer.write("\n")?;

        // pointers and messages
        for i in (0..messages.len()).rev() {
            written += Self::write_line_prefix(None, max_lineno_length, writer)?
                + Self::write_ptr_lines(&messages[..i + 1], writer)?;
            if let Some((color, message)) = messages.get(i).map(|v| (v.color(), v.message())) {
                written += Style::from(color).bold().write(writer)?
                    + writer.write(message)?
                    + Style::default().reset().write(writer)?;
            }
            written += writer.write("\n")?;
        }

        Ok(written)
    }

    fn write_ptr_lines(
        messages: &[Box<dyn ReportInlineMessage>],
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        let mut written = 0;
        let mut ptr = 1;
        for message in messages.iter().take(messages.len().saturating_sub(1)) {
            written += Style::from(message.color()).bold().write(writer)?
                + writer.write_rep(' ', message.ptr().col().saturating_sub(ptr))?
                + writer.write("|")?
                + Style::default().reset().write(writer)?;
            ptr = message.ptr().col() + 1;
        }
        let space = match messages.len() {
            0 => 0,
            1 => *messages[0].ptr().col(),
            l @ 2.. => messages[l - 1]
                .ptr()
                .col()
                .saturating_sub(*messages[l - 2].ptr().col()),
        }
        .saturating_sub(1);
        written += writer.write_rep(' ', space)?;
        Ok(written)
    }

    fn write_line_prefix(
        line_number: Option<usize>,
        number_length: usize,
        writer: &mut impl DisplayWriter,
    ) -> Result<usize, IoError> {
        Ok(Self::prefix_style().write(writer)?
            + match line_number {
                Some(line_number) => {
                    let line_number = line_number.to_string();
                    writer.write_rep(' ', line_number.len().saturating_sub(number_length))?
                        + writer.write(line_number)?
                }
                None => writer.write_rep(' ', number_length)?,
            }
            + writer.write(" | ")?
            + Style::default().reset().write(writer)?)
    }

    fn prefix_style() -> Style {
        Style::from(Color::Blue).bold()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrimitiveMainMessage {
    message: String,
    display_type: String,
    color: Color,
    id: Option<String>,
}

impl PrimitiveMainMessage {
    pub fn new(message: String, display_type: String, color: Color, id: Option<String>) -> Self {
        Self {
            message,
            display_type,
            color,
            id,
        }
    }

    pub fn error(message: String, id: String) -> Self {
        Self::new(message, "error".to_string(), Color::Red, Some(id))
    }

    pub fn warning(message: String, id: String) -> Self {
        Self::new(message, "warning".to_string(), Color::Yellow, Some(id))
    }
}

impl ReportSegmentMesssage for PrimitiveMainMessage {
    fn message(&self) -> &String {
        &self.message
    }

    fn display_type(&self) -> &String {
        &self.display_type
    }

    fn color(&self) -> &Color {
        &self.color
    }

    fn id(&self) -> &Option<String> {
        &self.id
    }
}

#[derive(Debug)]
pub struct PrimitiveReport {
    segments: Vec<Box<dyn ReportSegment>>,
}

impl PrimitiveReport {
    pub fn new(segments: Vec<Box<dyn ReportSegment>>) -> Self {
        Self { segments }
    }

    pub fn single(segment: PrimitiveReportSegment) -> Self {
        Self::new(vec![Box::new(segment)])
    }
}

impl Report for PrimitiveReport {
    fn segments(&self) -> &[Box<dyn ReportSegment>] {
        &self.segments
    }
}

#[derive(Debug)]
pub struct PrimitiveReportSegment {
    main_message: Option<Box<dyn ReportSegmentMesssage>>,
    messages: Vec<Box<dyn ReportInlineMessage>>,
}

impl PrimitiveReportSegment {
    pub fn new(
        main_message: Option<PrimitiveMainMessage>,
        messages: Vec<Box<dyn ReportInlineMessage>>,
    ) -> Self {
        Self {
            main_message: main_message.map(|v| Box::new(v) as Box<dyn ReportSegmentMesssage>),
            messages,
        }
    }

    pub fn single(main_message: PrimitiveMainMessage, message: PrimitiveReportMessage) -> Self {
        Self::new(Some(main_message), vec![Box::new(message)])
    }
}

impl ReportSegment for PrimitiveReportSegment {
    fn main_message(&self) -> &Option<Box<dyn ReportSegmentMesssage>> {
        &self.main_message
    }

    fn inline_messages(&self) -> &[Box<dyn ReportInlineMessage>] {
        &self.messages
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrimitiveReportMessage {
    message: String,
    color: Color,
    underline_symbol: char,
    span: usize,
    ptr: Pos,
}

impl PrimitiveReportMessage {
    pub fn new(
        message: String,
        color: Color,
        underline_symbol: char,
        span: usize,
        ptr: Pos,
    ) -> Self {
        Self {
            message,
            color,
            underline_symbol,
            span,
            ptr,
        }
    }

    pub fn error(message: String, span: usize, ptr: Pos) -> Self {
        Self::new(message, Color::Red, '^', span, ptr)
    }

    pub fn warning(message: String, span: usize, ptr: Pos) -> Self {
        Self::new(message, Color::Yellow, '^', span, ptr)
    }
}

impl ReportInlineMessage for PrimitiveReportMessage {
    fn message(&self) -> &str {
        &self.message
    }

    fn color(&self) -> &Color {
        &self.color
    }

    fn span(&self) -> &usize {
        &self.span
    }

    fn ptr(&self) -> &dyn ReportTextPointer {
        &self.ptr
    }

    fn underline_symbol(&self) -> &char {
        &self.underline_symbol
    }
}
