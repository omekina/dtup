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
    /// `#`
    Hash,
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
    Numeric(NumericLiteral),
    String(String),
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
            }
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
            v @ ('a'..='z' | 'A'..='Z') => {
                let v = try_result!(consume_ident(v, self.source));
                (Token::Ident(v.0), v.1)
            }

            v @ (' ' | '\t' | '\n') => {
                let count = try_result!(count_matching(&v, self.source)) + 1;
                let of_type: WhitespaceTokenType = v.try_into().unwrap();
                (Token::Whitespace(WhitespaceToken { of_type, count }), count)
            }

            v @ '0'..='9' => {
                self.source.push(StreamResult::Ok(v));
                match consume_numeric_literal(self.source) {
                    StreamResult::Ok(v) => {
                        return Some(StreamResult::Ok(SpanToken {
                            token: Token::Literal(LiteralToken::Numeric(v.0)),
                            span: v.1,
                        }));
                    }
                    StreamResult::IoError(e) => return Some(StreamResult::IoError(e)),
                    StreamResult::ProcessingError(e) => {
                        return Some(StreamResult::ProcessingError(e));
                    }
                }
            }

            '"' => match consume_string(self.source, ptr) {
                StreamResult::Ok((v, pos, span)) => {
                    return Some(StreamResult::Ok(SpanToken { span: Span {
                        ptr: pos, span,
                    }, token: Token::Literal(LiteralToken::String(v)) }))
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

            '#' => (Token::Hash, 1),
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
            Some(StreamResult::ProcessingError(e)) => return StreamResult::ProcessingError(
                StreamedError::ShouldEnd(e)
            ),
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
            },
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
            '*' => {
                match try_yield!(source.next()) {
                    Some('/') => {
                        terminated = true;
                        break;
                    },
                    None => break,
                    Some(v @ _) => {
                        source.push(SourceResult::Ok(v));
                    }
                }
            }
            v @ _ => {
                len += 1;
                res.push(v);
            }
        }
    }
    match terminated {
        true => TokenizerResult::Ok(SpanToken {
            span: Span { ptr: start, span: len },
            token: Token::Comment(Comment { of_type: CommentType::Block, content: res }),
        }),
        false => TokenizerResult::ProcessingError(StreamedError::CanContinue(simple_error(
            Errors::UnclosedBlockComment,
            "unclosed block comment begins here".to_string(),
            2,
            start,
        ))),
    }
}

fn consume_ident<I>(prepend: char, source: &mut I) -> SourceResult<(String, usize)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>,
{
    let mut len = 1;
    let mut res = String::new();
    res.push(prepend);
    while let Some(c) = source.next() {
        let c = match c.into() {
            Ok(v) => v,
            Err(e) => return e,
        };
        match c {
            '0'..='9' | 'a'..='z' | 'A'..='Z' | ',' | '.' | '_' | '+' | '-' => res.push(c),
            _ => {
                source.push(StreamResult::Ok(c));
                break;
            }
        }
        len += 1;
    }
    StreamResult::Ok((res, len))
}

fn consume_string<I>(source: &mut I, start: Pos) -> TokenizerResult<(String, Pos, usize)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>>
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
                    1, start,
                )))
            }
            v @ _ => res.push(v),
        }
    }

    match terminated {
        true => StreamResult::Ok((res, start, len)),
        false => StreamResult::ProcessingError(StreamedError::CanContinue(simple_error(
            Errors::InvalidStringLiteral,
            "unterminated string literal begins here".to_string(),
            1, start,
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

enum GenericNumericLiteralResult {
    Add(u64),
    InvalidSymbol(String),
    End,
}

fn consume_generic_numeric_literal<I>(
    prepend: &[char],
    prepend_start_pos: Pos,
    init_span: usize,
    source: &mut I,
    base: u64,
    mapper: impl Fn(&char) -> GenericNumericLiteralResult,
    numeric_type: NumericLiteralType,
) -> TokenizerResult<(NumericLiteral, Span)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    let mut len = init_span;
    let mut res = 0u64;

    macro_rules! throw {
        (@skip) => {
            match skip_while(source, |c| {
                match c {
                    '0'..='9' | 'a'..='z' | 'A'..='Z' => true,
                    _ => false,
                }
            }) {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(e) => {
                    return StreamResult::ProcessingError(StreamedError::ShouldEnd(e));
                }
            }
        };

        (@err $e: expr, $span: expr, $ptr: expr) => {
            StreamResult::ProcessingError(StreamedError::CanContinue(simple_error(
                Errors::InvalidNumericLiteral,
                $e,
                $span,
                $ptr,
            )))
        };

        ($e: expr) => {{
            let skipped = throw!(@skip) + 1;
            return throw!(@err $e, len + skipped, prepend_start_pos);
        }};

        (exact $e: expr) => {{
            let ptr = source.last_ptr();
            throw!(@skip);
            return throw!(@err $e, 1, ptr);
        }};
    }

    macro_rules! add {
        ($to_add: expr, $target: expr) => {
            match $target.checked_mul(base).map(|v| v.checked_add($to_add)) {
                Some(Some(v)) => v,
                _ => throw!(format!(
                    "numeric literal too large, maximum is `{}`",
                    u64::MAX
                )),
            }
        };
    }

    for c in prepend {
        match mapper(c) {
            GenericNumericLiteralResult::Add(to_add) => {
                len += 1;
                res = add!(to_add, res);
            }
            GenericNumericLiteralResult::InvalidSymbol(msg) => throw!(exact msg),
            GenericNumericLiteralResult::End => panic!("invalid numeric literal prefix handling"),
        }
    }

    while let Some(v) = source.next() {
        let c = match v {
            StreamResult::Ok(v) => v,
            v @ _ => {
                source.push(v);
                break;
            }
        };
        match mapper(&c) {
            GenericNumericLiteralResult::Add(to_add) => {
                len += 1;
                res = add!(to_add, res);
            }
            GenericNumericLiteralResult::InvalidSymbol(msg) => throw!(exact msg),
            GenericNumericLiteralResult::End => {
                source.push(v);
                break;
            }
        }
    }
    StreamResult::Ok((
        NumericLiteral {
            of_type: numeric_type,
            value: res,
        },
        Span {
            ptr: prepend_start_pos,
            span: len,
        },
    ))
}

fn consume_decimal_literal<I>(
    prepend: &[char],
    prepend_start_pos: Pos,
    init_span: usize,
    source: &mut I,
) -> TokenizerResult<(NumericLiteral, Span)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    consume_generic_numeric_literal(
        prepend,
        prepend_start_pos,
        init_span,
        source,
        10,
        |c| match c {
            '0' => GenericNumericLiteralResult::Add(0),
            '1' => GenericNumericLiteralResult::Add(1),
            '2' => GenericNumericLiteralResult::Add(2),
            '3' => GenericNumericLiteralResult::Add(3),
            '4' => GenericNumericLiteralResult::Add(4),
            '5' => GenericNumericLiteralResult::Add(5),
            '6' => GenericNumericLiteralResult::Add(6),
            '7' => GenericNumericLiteralResult::Add(7),
            '8' => GenericNumericLiteralResult::Add(8),
            '9' => GenericNumericLiteralResult::Add(9),
            'a'..='f' | 'A'..='F' => GenericNumericLiteralResult::InvalidSymbol(
                "invalid symbol for a decimal literal, use `0x` prefix for hexadecimal".to_string(),
            ),
            'g'..='z' | 'G'..='Z' => GenericNumericLiteralResult::InvalidSymbol(
                "invalid symbol for a decimal literal".to_string(),
            ),
            _ => GenericNumericLiteralResult::End,
        },
        NumericLiteralType::Decimal,
    )
}

fn consume_hexadecimal_literal<I>(
    prepend: &[char],
    prepend_start_pos: Pos,
    init_span: usize,
    source: &mut I,
) -> TokenizerResult<(NumericLiteral, Span)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    consume_generic_numeric_literal(
        prepend,
        prepend_start_pos,
        init_span,
        source,
        16,
        |c| match c {
            '0' => GenericNumericLiteralResult::Add(0),
            '1' => GenericNumericLiteralResult::Add(1),
            '2' => GenericNumericLiteralResult::Add(2),
            '3' => GenericNumericLiteralResult::Add(3),
            '4' => GenericNumericLiteralResult::Add(4),
            '5' => GenericNumericLiteralResult::Add(5),
            '6' => GenericNumericLiteralResult::Add(6),
            '7' => GenericNumericLiteralResult::Add(7),
            '8' => GenericNumericLiteralResult::Add(8),
            '9' => GenericNumericLiteralResult::Add(9),
            'a' | 'A' => GenericNumericLiteralResult::Add(10),
            'b' | 'B' => GenericNumericLiteralResult::Add(11),
            'c' | 'C' => GenericNumericLiteralResult::Add(12),
            'd' | 'D' => GenericNumericLiteralResult::Add(13),
            'e' | 'E' => GenericNumericLiteralResult::Add(14),
            'f' | 'F' => GenericNumericLiteralResult::Add(15),
            'g'..='z' | 'G'..='Z' => GenericNumericLiteralResult::InvalidSymbol(
                "invalid symbol for a hexadecimal literal".to_string(),
            ),
            _ => GenericNumericLiteralResult::End,
        },
        NumericLiteralType::Hex,
    )
}

fn consume_numeric_literal<I>(source: &mut I) -> TokenizerResult<(NumericLiteral, Span)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    macro_rules! try_result {
        ($v: expr) => {
            match $v {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(e) => {
                    return StreamResult::ProcessingError(StreamedError::ShouldEnd(e))
                }
            }
        };
    }
    match try_result!(source.next().unwrap()) {
        '0' => {
            let ptr = source.last_ptr();
            match source.next() {
                Some(v) => match try_result!(v) {
                    'x' => consume_hexadecimal_literal(&[], ptr, 2, source),
                    v @ _ => {
                        source.push(StreamResult::Ok(v));
                        consume_decimal_literal(&['0'], ptr, 1, source)
                    }
                },
                None => StreamResult::Ok((
                    NumericLiteral {
                        of_type: NumericLiteralType::Decimal,
                        value: 0,
                    },
                    Span { ptr, span: 1 },
                )),
            }
        }
        v @ _ => consume_decimal_literal(&[v], source.last_ptr(), 1, source),
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
