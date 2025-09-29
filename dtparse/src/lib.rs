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

mod report;
mod styling;
mod string;
