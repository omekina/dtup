use crate::{
    pointer_stream::Pos,
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::{PrependablePointer, StreamPrepend},
};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Default))]
pub struct Span {
    pub ptr: Pos,
    pub span: usize,
}

#[derive(Debug)]
pub struct SpanToken {
    pub span: Span,
    pub token: Token,
}

#[derive(Debug)]
pub enum Token {
    Whitespace(WhitespaceToken),
    Literal(LiteralToken),
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
    /// `.`
    Period,
    /// `&`
    Ampersand,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `{` or `[` or `(`
    GroupOpening(GroupType),
    /// `}` or `]` or `)`
    GroupClosing(GroupType),
    /// `@`
    At,
    /// `#`
    Hash,
    /// `?`
    QuestionMark,
    ArithmeticOperator(ArithmeticOperator),
    BitwiseOperator(BitwiseOperator),
    LogicalOperator(LogicalOperator),
    RelationalOperator(RelationalOperator),
}

impl From<Token> for String {
    fn from(value: Token) -> Self {
        match value {
            Token::Literal(lit) => lit.into(),
            Token::Comment(comment) => comment.to_string(),
            Token::Equal => "=".to_string(),
            Token::Semicolon => ";".to_string(),
            Token::Colon => ":".to_string(),
            Token::Slash => "/".to_string(),
            Token::Comma => ",".to_string(),
            Token::Period => ".".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Lt => "<".to_string(),
            Token::Gt => ">".to_string(),
            Token::GroupOpening(of_type) => of_type.start_delimiter().to_string(),
            Token::GroupClosing(of_type) => of_type.end_delimiter().to_string(),
            Token::At => "@".to_string(),
            Token::Hash => "#".to_string(),
            Token::QuestionMark => "?".to_string(),
            Token::ArithmeticOperator(operator) => <&'static str>::from(operator).to_string(),
            Token::BitwiseOperator(operator) => <&'static str>::from(operator).to_string(),
            Token::LogicalOperator(operator) => <&'static str>::from(operator).to_string(),
            Token::RelationalOperator(operator) => <&'static str>::from(operator).to_string(),
            Token::Whitespace(whitespace) => whitespace.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Comment {
    pub of_type: CommentType,
    pub content: String,
}

impl std::fmt::Display for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.of_type.start_delimiter(),
            self.content,
            self.of_type.end_delimiter()
        )
    }
}

#[derive(Debug)]
pub enum CommentType {
    Block,
    Line,
}

impl CommentType {
    fn start_delimiter(&self) -> &'static str {
        match self {
            Self::Block => "/*",
            Self::Line => "//",
        }
    }

    fn end_delimiter(&self) -> &'static str {
        match self {
            Self::Block => "*/",
            Self::Line => "",
        }
    }
}

#[derive(Debug)]
pub struct WhitespaceToken {
    pub of_type: WhitespaceTokenType,
    pub count: usize,
}

impl std::fmt::Display for WhitespaceToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for _ in 0..self.count {
            write!(f, "{}", self.of_type)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum GroupType {
    /// `{`, `}`
    Brace,
    /// `[`, `]`
    Square,
    /// `(`, `)`
    Paren,
}

impl GroupType {
    fn start_delimiter(&self) -> &'static str {
        match self {
            Self::Brace => "{",
            Self::Square => "[",
            Self::Paren => "(",
        }
    }

    fn end_delimiter(&self) -> &'static str {
        match self {
            Self::Brace => "}",
            Self::Square => "]",
            Self::Paren => ")",
        }
    }
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

impl std::fmt::Display for WhitespaceTokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space => write!(f, " "),
            Self::Tab => write!(f, "\t"),
            Self::Newline => writeln!(f),
        }
    }
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
    Ident(GenericLiteral),
    String(String),
}

impl From<LiteralToken> for String {
    fn from(value: LiteralToken) -> Self {
        match value {
            LiteralToken::Ident(lit) => lit.content,
            LiteralToken::String(str) => str,
        }
    }
}

#[derive(Debug)]
pub struct GenericLiteral {
    pub content: String,
    pub of_type: GenericLiteralType,
}

#[derive(Debug)]
pub enum GenericLiteralType {
    DecimalNumeric,
    HexadecimalNumeric { prefix: bool },
    Ident,
}

impl GenericLiteral {
    /// # Panics
    /// If the char is unknown
    fn empty_from_char(symbol: &char) -> Self {
        let of_type = match symbol {
            '0'..='9' => GenericLiteralType::DecimalNumeric,
            'a'..='f' | 'A'..='F' => GenericLiteralType::HexadecimalNumeric { prefix: false },
            'g'..='z' | 'G'..='Z' | '_' => GenericLiteralType::Ident,
            v @ _ => panic!("bad generic literal handling for symbol: {:?}", v),
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
                self.of_type = GenericLiteralType::Ident;
                true
            }
            (GenericLiteralType::Ident, '0'..='9' | 'a'..='z' | 'A'..='Z' | '_') => true,
            (_, _) => false,
        };
        if res {
            self.content.push(*symbol);
        }
        res
    }

    /// # Error
    /// Returns the indices of the invalid symbols if the numeric literal is invalid
    pub fn req_number(self) -> Result<NumericLiteral, Vec<usize>> {
        match self.of_type {
            GenericLiteralType::DecimalNumeric => Ok(NumericLiteral::dec(self.content)),
            GenericLiteralType::HexadecimalNumeric { prefix } => {
                Ok(NumericLiteral::hex(self.content, prefix))
            }
            GenericLiteralType::Ident => Err({
                let mut found_idcs = Vec::new();
                for (idx, c) in self.content.chars().enumerate() {
                    match c {
                        '0'..='9' | 'a'..='f' | 'A'..='F' => {}
                        _ => found_idcs.push(idx),
                    }
                }
                found_idcs
            }),
        }
    }

    pub fn req_ident(self) -> String {
        self.content
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
    content: String,
}

impl NumericLiteral {
    fn new(of_type: NumericLiteralType, content: String) -> Self {
        Self { of_type, content }
    }

    fn dec(content: String) -> Self {
        Self::new(NumericLiteralType::Decimal, content)
    }

    fn hex(content: String, with_prefix: bool) -> Self {
        Self::new(NumericLiteralType::Hex { with_prefix }, content)
    }
}

impl From<NumericLiteral> for String {
    fn from(value: NumericLiteral) -> Self {
        value.content
    }
}

#[derive(Debug)]
pub enum NumericLiteralType {
    Hex { with_prefix: bool },
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

impl From<ArithmeticOperator> for &'static str {
    fn from(value: ArithmeticOperator) -> Self {
        match value {
            ArithmeticOperator::Plus => "+",
            ArithmeticOperator::Dash => "-",
            ArithmeticOperator::Asterisk => "*",
            ArithmeticOperator::Percent => "%",
        }
    }
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

impl From<BitwiseOperator> for &'static str {
    fn from(value: BitwiseOperator) -> Self {
        match value {
            BitwiseOperator::Or => "|",
            BitwiseOperator::Xor => "^",
            BitwiseOperator::Not => "~",
            BitwiseOperator::LeftShift => "<<",
            BitwiseOperator::RightShift => ">>",
        }
    }
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

impl From<LogicalOperator> for &'static str {
    fn from(value: LogicalOperator) -> Self {
        match value {
            LogicalOperator::And => "&&",
            LogicalOperator::Or => "||",
            LogicalOperator::Not => "!",
        }
    }
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

impl From<RelationalOperator> for &'static str {
    fn from(value: RelationalOperator) -> Self {
        match value {
            RelationalOperator::LessOrEqual => "<=",
            RelationalOperator::GreaterOrEqual => ">=",
            RelationalOperator::Equal => "==",
            RelationalOperator::NotEqual => "!=",
        }
    }
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
            v @ ('a'..='z' | 'A'..='Z' | '0'..='9' | '_') => {
                let v = try_result!(consume_generic_literal(v, self.source));
                (Token::Literal(LiteralToken::Ident(v.0)), v.1)
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

            '{' => (Token::GroupOpening(GroupType::Brace), 1),
            '}' => (Token::GroupClosing(GroupType::Brace), 1),
            '(' => (Token::GroupOpening(GroupType::Paren), 1),
            ')' => (Token::GroupClosing(GroupType::Paren), 1),
            '[' => (Token::GroupOpening(GroupType::Square), 1),
            ']' => (Token::GroupClosing(GroupType::Square), 1),

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
            '.' => (Token::Period, 1),
            '@' => (Token::At, 1),
            '#' => (Token::Hash, 1),
            '?' => (Token::QuestionMark, 1),
            v => {
                return Some(StreamResult::ProcessingError(StreamedError::CanContinue(
                    simple_error(
                        Errors::InvalidCharacter,
                        format!("unknown character {:?} is here", v),
                        1,
                        self.source.last_ptr(),
                    ),
                )));
            }
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
    let mut len = 2;
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

pub struct ErrorSkipper<'a, I> {
    source: &'a mut I,
    errors: Vec<ParseErrorReport>,
    has_ended: bool,
}

impl<'a, I> ErrorSkipper<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self {
            source,
            errors: Vec::default(),
            has_ended: false,
        }
    }
}

impl<I> ErrorSkipper<'_, I> {
    pub fn take_errors(&mut self) -> Vec<ParseErrorReport> {
        std::mem::take(&mut self.errors)
    }
}

impl<I> Iterator for ErrorSkipper<'_, I>
where
    I: Iterator<Item = TokenizerResult<SpanToken>>,
{
    type Item = SourceResult<SpanToken>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.has_ended {
            return None;
        }
        loop {
            match self.source.next() {
                Some(StreamResult::Ok(v)) => break Some(StreamResult::Ok(v)),
                Some(StreamResult::IoError(e)) => break Some(StreamResult::IoError(e)),
                Some(StreamResult::ProcessingError(StreamedError::CanContinue(e))) => {
                    self.errors.push(e);
                }
                Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(e))) => {
                    break Some(StreamResult::ProcessingError(e));
                }
                None => {
                    self.has_ended = true;
                    None?
                }
            }
        }
    }
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
