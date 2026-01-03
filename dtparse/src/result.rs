use crate::report::Report;

pub enum Errors {
    /// An invalid byte was encountered - resulting in an invalid UTF-8 character.
    InvalidUtf8Character,
}

impl Errors {
    pub fn id(&self) -> String {
        match self {
            Self::InvalidUtf8Character => "E001".to_string(),
        }
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
    pub fn map<B>(self, mapper: impl Fn(A) -> B) -> StreamResult<B, E> {
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
