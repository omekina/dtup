use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use crate::{MaybeOwnedString, report::ReportTextPointer, result::IoError};

pub trait FileReader {
    /// This must not return any newlines in the result
    fn read_line_lossy<'a>(
        &'a mut self,
        line: &dyn ReportTextPointer,
    ) -> Result<(MaybeOwnedString<'a>, usize), IoError>;
}

#[derive(Debug)]
pub enum BasicFileReaderError {
    OpenError(std::io::Error),
    FSeekError(std::io::Error),
    ReadLine(std::io::Error),
}

impl std::fmt::Display for BasicFileReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::OpenError(_) => "error opening file",
                Self::FSeekError(_) => "error fseeking",
                Self::ReadLine(_) => "error reading the current line",
            }
        )
    }
}

impl std::error::Error for BasicFileReaderError {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        match self {
            Self::OpenError(e) => Some(e),
            Self::FSeekError(e) => Some(e),
            Self::ReadLine(e) => Some(e),
        }
    }

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpenError(e) => Some(e),
            Self::FSeekError(e) => Some(e),
            Self::ReadLine(e) => Some(e),
        }
    }
}

#[derive(Debug, Default)]
pub struct BasicFileReader {
    handles: HashMap<PathBuf, File>,
}

impl BasicFileReader {
    fn open(file: &Path) -> Result<File, IoError> {
        Ok(File::open(file).map_err(BasicFileReaderError::OpenError)?)
    }

    fn seek(file: &mut File, offset: usize) -> Result<(), IoError> {
        file.seek(SeekFrom::Start(offset.try_into()?))
            .map_err(BasicFileReaderError::FSeekError)?;
        Ok(())
    }

    fn read_line(file: &mut File) -> Result<(String, usize), IoError> {
        let read =
            Self::read_until(file, |b| b != b'\n').map_err(BasicFileReaderError::ReadLine)?;
        let str = String::from_utf8_lossy(&read);
        Ok((str.to_string(), str.chars().count()))
    }

    fn read_until(
        file: &mut File,
        predicate: impl Fn(u8) -> bool,
    ) -> Result<Vec<u8>, std::io::Error> {
        let mut res = Vec::new();
        let mut buf = [0; 1024];
        let mut read = file.read(&mut buf)?;
        while read > 0 {
            let mut trim_to = read;
            for (idx, item) in buf.iter().enumerate().take(read) {
                if !predicate(*item) {
                    trim_to = idx;
                    break;
                }
            }
            res.extend(
                buf[0..trim_to]
                    .iter()
                    .map(|v| if v.is_ascii_control() { b' ' } else { *v }),
            );
            read = file.read(&mut buf)?;
        }
        Ok(res)
    }
}

impl FileReader for BasicFileReader {
    fn read_line_lossy<'a>(
        &'a mut self,
        line: &dyn ReportTextPointer,
    ) -> Result<(MaybeOwnedString<'a>, usize), IoError> {
        let file = line.file();
        let file = match self.handles.get_mut(file) {
            Some(v) => v,
            None => {
                self.handles.insert(file.to_path_buf(), Self::open(file)?);
                self.handles.get_mut(file).unwrap()
            }
        };
        Self::seek(file, *line.line_start_byte())?;
        let (read, read_count) = Self::read_line(file)?;
        Ok((MaybeOwnedString::Owned(read), read_count))
    }
}

#[derive(Debug)]
pub enum BasicFileStreamerError {
    OpenError(std::io::Error),
    ReadError(std::io::Error),
}

impl From<BasicFileStreamerError> for Box<dyn std::error::Error + Send> {
    fn from(value: BasicFileStreamerError) -> Self {
        Box::new(value)
    }
}

impl std::fmt::Display for BasicFileStreamerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "basic_file_streamer_error: {}",
            match self {
                Self::OpenError(_) => "file open error",
                Self::ReadError(_) => "error while reading",
            }
        )
    }
}

impl std::error::Error for BasicFileStreamerError {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        match self {
            Self::OpenError(e) => Some(e),
            Self::ReadError(e) => Some(e),
        }
    }

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpenError(e) => Some(e),
            Self::ReadError(e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub struct BasicFileStreamer {
    file: File,
    ptr: usize,
    read: usize,
    buf: [u8; 1024],
}

impl BasicFileStreamer {
    pub fn new(file: &Path) -> Result<Self, IoError> {
        Ok(Self {
            file: File::open(file).map_err(BasicFileStreamerError::OpenError)?,
            ptr: 0,
            read: 0,
            buf: [0; 1024],
        })
    }

    fn call_read(&mut self) -> Result<(), IoError> {
        self.read = self
            .file
            .read(&mut self.buf)
            .map_err(BasicFileStreamerError::ReadError)?;
        self.ptr = 0;
        Ok(())
    }

    fn read_byte(&mut self) -> Result<Option<u8>, IoError> {
        if self.ptr >= self.read {
            self.call_read()?;
        }
        if self.ptr < self.read {
            let res = self.buf[self.ptr];
            self.ptr += 1;
            Ok(Some(res))
        } else {
            Ok(None)
        }
    }
}

impl Iterator for BasicFileStreamer {
    type Item = Result<u8, IoError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_byte() {
            Ok(Some(b)) => Some(Ok(b)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
