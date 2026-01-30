use crate::report::Report;

pub enum Errors {
    /// An invalid byte was encountered - resulting in an invalid UTF-8 character.
    InvalidUtf8Character,
    /// Unknown character
    InvalidCharacter,
    /// Invalid symbol inside a numeric literal
    InvalidNumericLiteral,
    InvalidStringLiteral,
    UnclosedBlockComment,
    UnexpectedEof,
    UnexpectedToken,
    InvalidNodeAddress,
    InvalidNodeName,
    InvalidLabelName,
    UnexpectedWhitespace,
    MissingParentheses,
    UnmatchedDelimiter,
}

impl Errors {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidUtf8Character => "invalid UTF-8 character",
            Self::InvalidCharacter => "Unknown character encountered",
            Self::InvalidNumericLiteral => "invalid numeric literal",
            Self::InvalidStringLiteral => "invalid string literal",
            Self::UnclosedBlockComment => "unclosed block comment",
            Self::UnexpectedEof => "unxpected end",
            Self::UnexpectedToken => "unexpected token",
            Self::InvalidNodeAddress => "invalid node address",
            Self::InvalidNodeName => "invalid node name",
            Self::InvalidLabelName => "invalid label name",
            Self::UnexpectedWhitespace => "unexpected whitespace or comment",
            Self::MissingParentheses => "missing parentheses",
            Self::UnmatchedDelimiter => "unmatched delimiter",
        }
        .to_string()
    }

    pub fn id(&self) -> String {
        match self {
            Self::InvalidUtf8Character => "E001",
            Self::InvalidCharacter => "E002",
            Self::InvalidNumericLiteral => "E003",
            Self::InvalidStringLiteral => "E004",
            Self::UnclosedBlockComment => "E005",
            Self::UnexpectedEof => "E006",
            Self::UnexpectedToken => "E007",
            Self::InvalidNodeAddress => "E008",
            Self::InvalidNodeName => "E009",
            Self::InvalidLabelName => "E010",
            Self::UnexpectedWhitespace => "E011",
            Self::MissingParentheses => "E012",
            Self::UnmatchedDelimiter => "E013",
        }
        .to_string()
    }
}

pub enum Warnings {
    WeirdPropertyName,
}

impl Warnings {
    pub fn message(&self) -> String {
        match self {
            Self::WeirdPropertyName => "weird property name",
        }
        .to_string()
    }

    pub fn id(&self) -> String {
        match self {
            Self::WeirdPropertyName => "W001",
        }
        .to_string()
    }
}

#[derive(Debug)]
pub struct IoError {
    error: Box<dyn std::error::Error>,
}

impl<E: std::error::Error + 'static> From<E> for IoError {
    fn from(value: E) -> Self {
        Self {
            error: Box::new(value),
        }
    }
}

#[derive(Debug)]
pub enum StreamedError<E> {
    CanContinue(E),
    ShouldEnd(E),
}

pub type ParseErrorReport = Box<dyn Report>;
pub type StreamedErrorReport = StreamedError<Box<dyn crate::report::Report>>;

#[derive(Debug)]
pub enum StreamResult<T, E> {
    Ok(T),
    IoError(IoError),
    ProcessingError(E),
}

impl<A, E> StreamResult<A, E> {
    pub fn map<B>(self, mapper: impl FnOnce(A) -> B) -> StreamResult<B, E> {
        match self {
            Self::Ok(v) => StreamResult::Ok(mapper(v)),
            Self::IoError(e) => StreamResult::IoError(e),
            Self::ProcessingError(e) => StreamResult::ProcessingError(e),
        }
    }
}

impl<T, E, R> From<StreamResult<T, E>> for Result<T, StreamResult<R, E>> {
    fn from(value: StreamResult<T, E>) -> Self {
        match value {
            StreamResult::Ok(v) => Self::Ok(v),
            StreamResult::IoError(e) => Self::Err(StreamResult::IoError(e)),
            StreamResult::ProcessingError(e) => Self::Err(StreamResult::ProcessingError(e)),
        }
    }
}

impl<T, E> From<std::io::Result<T>> for StreamResult<T, E> {
    fn from(value: std::io::Result<T>) -> Self {
        match value {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::IoError(e.into()),
        }
    }
}

#[macro_export]
macro_rules! try_stream {
    ($v: expr) => {
        match $v {
            StreamResult::Ok(v) => v,
            v @ _ => return v,
        }
    };
}
