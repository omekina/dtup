use crate::{
    pointer_stream::Pos,
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::{PrependablePointer, StreamPrepend},
};

#[derive(Debug)]
pub struct SpanToken {
    ptr: Pos,
    span: usize,
    token: Token,
}

#[derive(Debug)]
pub enum Token {
    Whitespace(WhitespaceToken),
    Literal(LiteralToken),
    Ident(String),
    /// `#`
    Hash,
    /// `=`
    Equal,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
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
    /// `+` (add)
    Plus,
    /// `-` (subtract)
    Dash,
    /// `*` (multiply)
    Asterisk,
    /// `/` (divide)
    Slash,
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

            '+' => (Token::ArithmeticOperator(ArithmeticOperator::Plus), 1),
            '-' => (Token::ArithmeticOperator(ArithmeticOperator::Dash), 1),
            '*' => (Token::ArithmeticOperator(ArithmeticOperator::Asterisk), 1),
            '/' => (Token::ArithmeticOperator(ArithmeticOperator::Slash), 1),
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
            ptr,
            span: res.1,
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

fn consume_decimal_literal<I>(
    first_digit: char,
    first_digit_pos: Pos,
    source: &mut I,
) -> TokenizerResult<(NumericLiteral, Pos)>
where
    I: Iterator<Item = SourceResult<char>> + StreamPrepend<SourceResult<char>> + PrependablePointer,
{
    let mut len = 0;
    let mut res = 0u64;

    macro_rules! throw {
        (@skip) => {
            match skip_while(source, |v| match v {
                '0'..='9' | 'a'..='z' | 'A'..='Z' => true,
                _ => false,
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
            let skipped = throw!(@skip);
            return throw!(@err $e, len + skipped, first_digit_pos);
        }};

        (exact $e: expr) => {{
            let ptr = source.last_ptr();
            throw!(@skip);
            return throw!(@err $e, 1, ptr);
        }};
    }

    while let Some(c) = source.next() {
        match c {
            StreamResult::Ok(c @ '0'..='9') => {
                len += 1;
                res = match res
                    .checked_mul(10)
                    .map(|v| v.checked_add(u64::try_from(c).unwrap()))
                {
                    Some(Some(v)) => v,
                    _ => throw!(format!(
                        "numeric literal too large, maximum is `{}`",
                        u64::MAX
                    )),
                };
            }
            StreamResult::Ok('a'..='f' | 'A'..='F') => {
                throw!(exact "invalid symbol in a decimal literal, use `0x` prefix for hexadecimal".to_string());
            }
            StreamResult::Ok(v @ ('g'..='z' | 'G'..='Z')) => {
                throw!(exact format!("invalid character `{:?}` for a numeric literal", v));
            }
            v @ _ => {
                source.push(v);
                break;
            }
        }
    }
    StreamResult::Ok((
        NumericLiteral {
            of_type: NumericLiteralType::Decimal,
            value: res,
        },
        first_digit_pos,
    ))
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
