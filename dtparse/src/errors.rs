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
