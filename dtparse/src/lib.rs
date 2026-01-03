mod result;

use result::IoError;

pub trait DisplayWriter {
    fn write(&mut self, to_write: impl AsRef<str>) -> Result<usize, IoError>;
    fn write_rep(&mut self, to_write: char, repeat: usize) -> Result<usize, IoError>;
}

impl DisplayWriter for std::io::BufWriter<std::io::Stdout> {
    fn write(&mut self, to_write: impl AsRef<str>) -> Result<usize, IoError> {
        Ok(std::io::Write::write(self, to_write.as_ref().as_bytes())?)
    }

    fn write_rep(&mut self, to_write: char, repeat: usize) -> Result<usize, IoError> {
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

mod file;
mod helpers;
mod pointer_stream;
mod report;
mod stream_utils;
mod string;
mod styling;
mod tokenizer;

pub use helpers::parse;
