pub trait DisplayWriter {
    fn write(&mut self, to_write: impl AsRef<str>) -> Result<usize, crate::Error>;
    fn write_rep(&mut self, to_write: char, repeat: usize) -> Result<usize, crate::Error>;
}

impl DisplayWriter for std::io::BufWriter<std::io::Stdout> {
    fn write(&mut self, to_write: impl AsRef<str>) -> Result<usize, crate::Error> {
        Ok(std::io::Write::write(self, to_write.as_ref().as_bytes())?)
    }

    fn write_rep(&mut self, to_write: char, repeat: usize) -> Result<usize, crate::Error> {
        let mut to_write_full = String::with_capacity(repeat * to_write.len_utf8());
        for _ in 0..repeat {
            to_write_full.push(to_write);
        }
        self.write(to_write_full)
    }
}

pub enum MaybeOwnedString<'a> {
    Referenced(&'a str),
    Owned(String),
}

impl MaybeOwnedString<'_> {
    pub fn len(&self) -> usize {
        match self {
            Self::Referenced(v) => v.len(),
            Self::Owned(v) => v.len(),
        }
    }
}

impl AsRef<str> for MaybeOwnedString<'_> {
    fn as_ref(&self) -> &str {
        match self {
            Self::Referenced(v) => v,
            Self::Owned(v) => &v,
        }
    }
}

pub type Error = Box<dyn std::error::Error>;

mod errors;
mod file;
mod pointer_stream;
mod report;
mod string;
mod styling;

pub use report::ReportDisplay;
pub use file::{BasicFileStreamer, BasicFileReader};
pub use string::StringDecoder;
pub use pointer_stream::{RawPointerTracker, PointerTracker};
