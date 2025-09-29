#[derive(PartialEq, Eq, Clone, Copy)]
enum Styles {
    Reset,
    Bold,
}

impl Styles {
    const fn value(&self) -> u8 {
        match self {
            Self::Reset => 0b1,
            Self::Bold => 0b10,
        }
    }
}

#[derive(Default, Clone)]
pub struct Style {
    styles: u8,
    color: Option<Color>,
}

impl Style {
    pub const fn contains(&self, style: Styles) -> bool {
        self.styles & style.value() > 0
    }

    pub const fn reset(mut self) -> Self {
        self.color = None;
        self.styles = Styles::Reset.value();
        self
    }

    pub const fn bold(mut self) -> Self {
        self.styles |= Styles::Bold.value();
        self
    }

    /// Write with this style
    pub fn write(
        &self,
        _writer: &mut impl crate::report::DisplayWriter,
    ) -> Result<usize, crate::Error> {
        Ok(0)
    }
}

impl From<Color> for Style {
    fn from(value: Color) -> Self {
        Self {
            color: Some(value),
            ..Default::default()
        }
    }
}

impl From<&Color> for Style {
    fn from(value: &Color) -> Self {
        Self {
            color: Some(*value),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Blue,
    Red,
}
