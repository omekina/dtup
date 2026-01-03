use crate::result::IoError;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Styles {
    Reset,
    Bold,
}

impl Styles {
    pub const fn value(&self) -> u8 {
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

    pub const fn remove_reset(&mut self) {
        self.styles &= 0b11111110;
    }

    pub const fn reset(mut self) -> Self {
        self.color = None;
        self.styles = Styles::Reset.value();
        self
    }

    pub const fn bold(mut self) -> Self {
        self.remove_reset();
        self.styles |= Styles::Bold.value();
        self
    }

    const fn ansi_inner(&self) -> &str {
        match self.styles {
            0b1 => "0",
            0b10 => "1",
            _ => unreachable!(),
        }
    }

    fn write_color(&self, writer: &mut impl crate::DisplayWriter) -> Result<usize, IoError> {
        Ok(match (self.color, self.styles & 0b1) {
            (Some(color), 0b0) => writer.write(";")? + writer.write(color.ansi_inner())?,
            _ => 0,
        })
    }

    /// Write with this style
    pub fn write(&self, writer: &mut impl crate::DisplayWriter) -> Result<usize, IoError> {
        Ok(writer.write("\x1b[")?
            + writer.write(self.ansi_inner())?
            + self.write_color(writer)?
            + writer.write("m")?)
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
    Red,
    Green,
    Yellow,
    Blue,
    Purple,
    Cyan,
}

impl Color {
    const fn ansi_inner(&self) -> &str {
        match self {
            Self::Red => "31",
            Self::Green => "32",
            Self::Yellow => "33",
            Self::Blue => "34",
            Self::Purple => "35",
            Self::Cyan => "36",
        }
    }
}
