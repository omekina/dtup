use crate::{
    pointer_stream::Pos,
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::{PrependablePointer, StreamPrepend},
};

#[derive(Debug)]
pub struct Span {
    pub ptr: Pos,
    pub span: usize,
}

#[derive(Debug)]
pub struct SpanToken {
    pub span: Span,
    token: Token,
}

#[derive(Debug)]
pub enum Token {
    Whitespace(WhitespaceToken),
    Literal(LiteralToken),
    Ident(String),
    Comment(Comment),
    /// `=`
    Equal,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `/`
    Slash,
    /// `,`
    Comma,
    /// `&`
    Ampersand,
    /// `<`
    Lt,
    /// `>`
    Gt,
    ArithmeticOperator(ArithmeticOperator),
    BitwiseOperator(BitwiseOperator),
    LogicalOperator(LogicalOperator),
    RelationalOperator(RelationalOperator),
}

#[derive(Debug)]
pub struct Comment {
    of_type: CommentType,
    content: String,
}

#[derive(Debug)]
pub enum CommentType {
    Block,
    Line,
}

#[derive(Debug)]
pub struct WhitespaceToken {
    of_type: WhitespaceTokenType,
    count: usize,
}

#[derive(Debug)]
pub enum WhitespaceTokenType {
    /// ` `
    Space,
    /// `\t`
    Tab,
    /// `\n`
    Newline,
}

impl TryFrom<char> for WhitespaceTokenType {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        Ok(match value {
            ' ' => Self::Space,
            '\t' => Self::Tab,
            '\n' => Self::Newline,
            _ => Err(())?,
        })
    }
}

#[derive(Debug)]
pub enum LiteralToken {
    Generic(GenericLiteral),
    String(String),
}

#[derive(Debug)]
pub struct GenericLiteral {
    content: String,
    of_type: GenericLiteralType,
}

#[derive(Debug)]
pub enum GenericLiteralType {
    DecimalNumeric,
    HexadecimalNumeric { prefix: bool },
    LabelName,
    NodeName,
    PropertyName,
}

impl GenericLiteral {
    /// # Panics
    /// If the char is unknown
    fn empty_from_char(symbol: &char) -> Self {
        let of_type = match symbol {
            '0'..='9' => GenericLiteralType::DecimalNumeric,
            'a'..='f' | 'A'..='F' => GenericLiteralType::HexadecimalNumeric { prefix: false },
            'g'..'z' | 'G'..='Z' | '_' => GenericLiteralType::LabelName,
            ',' | '.' | '+' | '-' => GenericLiteralType::NodeName,
            '?' | '#' => GenericLiteralType::PropertyName,
            _ => panic!("bad generic literal handling"),
        };
        Self {
            content: symbol.to_string(),
            of_type,
        }
    }

    fn hex_with_prefix() -> Self {
        Self {
            content: "0x".to_string(),
            of_type: GenericLiteralType::HexadecimalNumeric { prefix: true },
        }
    }

    fn advance(&mut self, symbol: &char) -> bool {
        let res = match (&self.of_type, symbol) {
            (GenericLiteralType::DecimalNumeric, '0'..='9') => true,
            (GenericLiteralType::DecimalNumeric, 'a'..='f' | 'A'..='F') => {
                self.of_type = GenericLiteralType::HexadecimalNumeric { prefix: false };
                true
            }
            (GenericLiteralType::HexadecimalNumeric { .. }, '0'..='9' | 'a'..='f' | 'A'..='F') => {
                true
            }
            (
                GenericLiteralType::DecimalNumeric | GenericLiteralType::HexadecimalNumeric { .. },
                'g'..='z' | 'G'..='Z' | '_',
            ) => {
                self.of_type = GenericLiteralType::LabelName;
                true
            }
            (GenericLiteralType::LabelName, '0'..='9' | 'a'..='z' | 'A'..='Z' | '_') => true,
            (
                GenericLiteralType::DecimalNumeric
                | GenericLiteralType::HexadecimalNumeric { .. }
                | GenericLiteralType::LabelName,
                ',' | '.' | '+' | '-',
            ) => {
                self.of_type = GenericLiteralType::NodeName;
                true
            }
            (
                GenericLiteralType::NodeName,
                '0'..='9' | 'a'..='z' | 'A'..='Z' | '_' | ',' | '.' | '+' | '-',
            ) => true,
            (
                GenericLiteralType::DecimalNumeric
                | GenericLiteralType::HexadecimalNumeric { .. }
                | GenericLiteralType::LabelName
                | GenericLiteralType::NodeName,
                '?' | '#',
            ) => {
                self.of_type = GenericLiteralType::PropertyName;
                true
            }
            (
                GenericLiteralType::PropertyName,
                '0'..='9' | 'a'..='z' | 'A'..='Z' | '_' | ',' | '.' | '+' | '-' | '?' | '#',
            ) => true,
            (_, _) => false,
        };
        if res {
            self.content.push(*symbol);
        }
        res
    }
}

enum GenericLiteralPrefixParser {
    Empty,
    ZeroStart,
    Content(GenericLiteral),
}

impl GenericLiteralPrefixParser {
    fn advance(&mut self, symbol: &char) -> bool {
        match (self, symbol) {
            (s @ Self::Empty, '0') => {
                *s = Self::ZeroStart;
                true
            }
            (s @ Self::Empty, v) => {
                *s = Self::Content(GenericLiteral::empty_from_char(v));
                true
            }
            (s @ Self::ZeroStart, 'x') => {
                *s = Self::Content(GenericLiteral::hex_with_prefix());
                true
            }
            (s @ Self::ZeroStart, v) => {
                let mut content = GenericLiteral::empty_from_char(&'0');
                let res = content.advance(v);
                *s = Self::Content(content);
                res
            }
            (Self::Content(c), v) => c.advance(v),
        }
    }

    fn finish(self) -> GenericLiteral {
        match self {
            Self::Empty => panic!("invalid generic literal builder handling"),
            Self::ZeroStart => GenericLiteral::empty_from_char(&'0'),
            Self::Content(c) => c,
        }
    }
}

#[derive(Debug)]
pub struct NumericLiteral {
    of_type: NumericLiteralType,
    value: u64,
}

#[derive(Debug)]
pub enum NumericLiteralType {
    Hex,
    Decimal,
}

#[derive(Debug)]
pub enum ArithmeticOperator {
    // `/` is omitted
    /// `+` (add)
    Plus,
    /// `-` (subtract)
    Dash,
    /// `*` (multiply)
    Asterisk,
    /// `%` (modulo)
    Percent,
}

#[derive(Debug)]
pub enum BitwiseOperator {
    // `&` is omitted
    /// `|`
    Or,
    /// `^`
    Xor,
    /// `~`
    Not,
    /// `<<`
    LeftShift,
    /// `>>`
    RightShift,
}

#[derive(Debug)]
pub enum LogicalOperator {
    /// `&&`
    And,
    /// `||`
    Or,
    /// `!`
    Not,
}

#[derive(Debug)]
pub enum RelationalOperator {
    // `<` and `>` are omitted
    /// `<=`
    LessOrEqual,
    /// `>=`
    GreaterOrEqual,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
}

type SourceResult<T> = StreamResult<T, ParseErrorReport>;
type TokenizerResult<T> = StreamResult<T, StreamedError<ParseErrorReport>>;

pub struct Tokenizer<'a, I> {
    source: &'a mut I,
}

impl<'a, I> Tokenizer<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self { source }
    }
}

impl<I> Iterator for Tokenizer<'_, I>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    type Item = TokenizerResult<SpanToken>;

    fn next(&mut self) -> Option<Self::Item> {
        macro_rules! try_result {
            ($v: expr) => {
                match $v {
                    StreamResult::Ok(v) => v,
                    StreamResult::IoError(e) => return Some(StreamResult::IoError(e)),
                    StreamResult::ProcessingError(e) => {
                        return Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(e)))
                    }
                }
            };

            (@direct $v: expr) => {
                match $v {
                    StreamResult::Ok(v) => v,
                    StreamResult::IoError(e) => return Some(StreamResult::IoError(e)),
                    StreamResult::ProcessingError(e) => {
                        return Some(StreamResult::ProcessingError(e))
                    }
                }
            };
        }

        let c = try_result!(self.source.next()?);
        let ptr = self.source.last_ptr();

        macro_rules! peek_yield {
            ($($peek_match: expr => $yield: expr),*$(,)? => $fallback_yield: expr) => {
                match self.source.next() {
                    $(Some(StreamResult::Ok($peek_match)) => ($yield, 2),)*
                    v @ _ => peek_yield!(@fallback v, $fallback_yield),
                }
            };

            (@custom $($peek_match: expr => $yield: expr),*$(,)? => $fallback_yield: expr) => {
                match self.source.next() {
                    $(Some(StreamResult::Ok($peek_match)) => $yield,)*
                    v @ _ => peek_yield!(@fallback v, $fallback_yield),
                }
            };

            (@fallback $v: expr, $token: expr) => {{
                if let Some(v) = $v {
                    self.source.push(v);
                }
                ($token, 1)
            }};
        }

        let res = match c {
            v @ ('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '#') => {
                let v = try_result!(consume_generic_literal(v, self.source));
                (Token::Literal(LiteralToken::Generic(v.0)), v.1)
            }

            v @ (' ' | '\t' | '\n') => {
                let count = try_result!(count_matching(&v, self.source)) + 1;
                let of_type: WhitespaceTokenType = v.try_into().unwrap();
                (Token::Whitespace(WhitespaceToken { of_type, count }), count)
            }

            '"' => match consume_string(self.source, ptr) {
                StreamResult::Ok((v, pos, span)) => {
                    return Some(StreamResult::Ok(SpanToken {
                        span: Span { ptr: pos, span },
                        token: Token::Literal(LiteralToken::String(v)),
                    }));
                }
                StreamResult::IoError(e) => return Some(StreamResult::IoError(e)),
                StreamResult::ProcessingError(e) => return Some(StreamResult::ProcessingError(e)),
            },

            '=' => peek_yield!(
                '=' => Token::RelationalOperator(RelationalOperator::Equal),
                => Token::Equal
            ),
            '&' => peek_yield!(
                '&' => Token::LogicalOperator(LogicalOperator::And),
                => Token::Ampersand
            ),
            '<' => peek_yield!(
                '<' => Token::BitwiseOperator(BitwiseOperator::LeftShift),
                '=' => Token::RelationalOperator(RelationalOperator::LessOrEqual),
                => Token::Lt
            ),
            '>' => peek_yield!(
                '>' => Token::BitwiseOperator(BitwiseOperator::RightShift),
                '=' => Token::RelationalOperator(RelationalOperator::GreaterOrEqual),
                => Token::Gt
            ),
            '|' => peek_yield!(
                '|' => Token::LogicalOperator(LogicalOperator::Or),
                => Token::BitwiseOperator(BitwiseOperator::Or)
            ),
            '/' => peek_yield!(@custom
                '/' => {
                    let (content, span) = try_result!(@direct consume_line_comment(self.source));
                    (Token::Comment(Comment { of_type: CommentType::Line, content }), span)
                },
                '*' => {
                    return Some(TokenizerResult::Ok(
                        try_result!(@direct consume_block_comment(ptr, self.source))
                    ));
                }
                => Token::Slash
            ),

            '+' => (Token::ArithmeticOperator(ArithmeticOperator::Plus), 1),
            '-' => (Token::ArithmeticOperator(ArithmeticOperator::Dash), 1),
            '*' => (Token::ArithmeticOperator(ArithmeticOperator::Asterisk), 1),
            '%' => (Token::ArithmeticOperator(ArithmeticOperator::Percent), 1),

            '^' => (Token::BitwiseOperator(BitwiseOperator::Xor), 1),
            '~' => (Token::BitwiseOperator(BitwiseOperator::Not), 1),

            '!' => (Token::LogicalOperator(LogicalOperator::Not), 1),
            ';' => (Token::Semicolon, 1),
            ':' => (Token::Colon, 1),
            ',' => (Token::Comma, 1),
            _ => todo!(),
        };
        Some(StreamResult::Ok(SpanToken {
            span: Span { ptr, span: res.1 },
            token: res.0,
        }))
    }
}

fn simple_error(error: Errors, message: String, span: usize, ptr: Pos) -> ParseErrorReport {
    Box::new(PrimitiveReport::single(PrimitiveReportSegment::single(
        PrimitiveMainMessage::error(error.message(), error.id()),
        PrimitiveReportMessage::error(message, span, ptr),
    )))
}

macro_rules! try_yield {
    ($v: expr) => {
        match $v {
            Some(StreamResult::Ok(v)) => Some(v),
            Some(StreamResult::IoError(e)) => return StreamResult::IoError(e),
            Some(StreamResult::ProcessingError(e)) => {
                return StreamResult::ProcessingError(StreamedError::ShouldEnd(e))
            }
            None => None,
        }
    };
}

fn consume_line_comment<I>(source: &mut I) -> TokenizerResult<(String, usize)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut len = 0;
    let mut res = String::new();
    while let Some(c) = try_yield!(source.next()) {
        match c {
            '\n' => {
                source.push(SourceResult::Ok(c));
                break;
            }
            v @ _ => {
                len += 1;
                res.push(v);
            }
        }
    }
    TokenizerResult::Ok((res, len))
}

fn consume_block_comment<I>(start: Pos, source: &mut I) -> TokenizerResult<SpanToken>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut len = 4;
    let mut res = String::new();
    let mut terminated = false;
    while let Some(c) = try_yield!(source.next()) {
        match c {
            '*' => match try_yield!(source.next()) {
                Some('/') => {
                    terminated = true;
                    break;
                }
                None => break,
                Some(v @ _) => {
                    source.push(SourceResult::Ok(v));
                }
            },
            v @ _ => {
                len += 1;
                res.push(v);
            }
        }
    }
    match terminated {
        true => TokenizerResult::Ok(SpanToken {
            span: Span {
                ptr: start,
                span: len,
            },
            token: Token::Comment(Comment {
                of_type: CommentType::Block,
                content: res,
            }),
        }),
        false => TokenizerResult::ProcessingError(StreamedError::CanContinue(simple_error(
            Errors::UnclosedBlockComment,
            "unclosed block comment begins here".to_string(),
            2,
            start,
        ))),
    }
}

fn consume_generic_literal<I>(
    prepend: char,
    source: &mut I,
) -> SourceResult<(GenericLiteral, usize)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut len = 1;
    let mut builder = GenericLiteralPrefixParser::Empty;
    if !builder.advance(&prepend) {
        panic!("consume generic literal called incorrectly");
    }
    while let Some(c) = source.next() {
        let c = match c {
            SourceResult::Ok(v) => v,
            SourceResult::IoError(e) => return SourceResult::IoError(e),
            SourceResult::ProcessingError(e) => return SourceResult::ProcessingError(e),
        };
        if !builder.advance(&c) {
            source.push(SourceResult::Ok(c));
            break;
        }
        len += 1;
    }
    SourceResult::Ok((builder.finish(), len))
}

fn consume_string<I>(source: &mut I, start: Pos) -> TokenizerResult<(String, Pos, usize)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut res = String::new();
    let mut len = 1;
    let mut terminated = false;
    while let Some(c) = try_yield!(source.next()) {
        len += 1;
        match c {
            '"' => {
                terminated = true;
                break;
            }
            '\n' => {
                return StreamResult::ProcessingError(StreamedError::CanContinue(simple_error(
                    Errors::InvalidStringLiteral,
                    "string literal started here contains a newline".to_string(),
                    1,
                    start,
                )));
            }
            v @ _ => res.push(v),
        }
    }

    match terminated {
        true => StreamResult::Ok((res, start, len)),
        false => StreamResult::ProcessingError(StreamedError::CanContinue(simple_error(
            Errors::InvalidStringLiteral,
            "unterminated string literal begins here".to_string(),
            1,
            start,
        ))),
    }
}

fn skip_while<T, I>(source: &mut I, predicate: impl Fn(&T) -> bool) -> SourceResult<usize>
where
    I: Iterator<Item = SourceResult<T>> + StreamPrepend<SourceResult<T>>,
{
    let mut res = 0;
    while let Some(v) = source.next() {
        let v = match v.into() {
            Ok(v) => v,
            Err(e) => return e,
        };
        if predicate(&v) {
            res += 1;
        } else {
            source.push(StreamResult::Ok(v));
            break;
        }
    }
    StreamResult::Ok(res)
}

fn count_matching<I>(target: &char, source: &mut I) -> SourceResult<usize>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut res = 0;
    while let Some(c) = source.next() {
        let c: char = match c.into() {
            Ok(v) => v,
            Err(e) => return e,
        };
        if c == *target {
            res += 1;
        } else {
            source.push(StreamResult::Ok(c));
            break;
        }
    }
    StreamResult::Ok(res)
}

#[cfg(test)]
mod test {
    use crate::{
        result::StreamResult,
        stream_utils::PrependableStream,
        tokenizer::{SourceResult, count_matching},
    };

    #[test]
    fn count_repeated_symbol() {
        let mut source: PrependableStream<SourceResult<char>, _, 1> =
            PrependableStream::new("----".chars().map(|v| StreamResult::Ok(v)));
        assert_eq!(
            match count_matching(&'-', &mut source) {
                StreamResult::Ok(v) => v,
                _ => panic!(),
            },
            4
        );
    }
}
