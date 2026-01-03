use crate::{
    pointer_stream::Pos,
    result::{ParseErrorReport, StreamResult},
    stream_utils::StreamPrepend,
};

pub struct SpanToken {
    ptr: Pos,
    span: usize,
}

#[derive(Debug)]
pub enum Token {
    Whitespace(WhitespaceToken),
    Literal(LiteralToken),
    Ident(String),
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
    ArithmeticOperator(ArithmeticOperator),
    BitwiseOperator(BitwiseOperator),
    LogicalOperator(LogicalOperator),
    RelationalOperator(RelationalOperator),
}

#[derive(Debug)]
pub enum WhitespaceToken {
    /// ` `
    Space,
    /// `\t`
    Tab,
    /// `\n`
    Newline,
}

#[derive(Debug)]
pub enum LiteralToken {
    Numeric(),
    String(String),
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

type TokenizerResult<T> = StreamResult<T, ParseErrorReport>;

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
    I: Iterator<Item = TokenizerResult<char>> + StreamPrepend<TokenizerResult<char>>,
{
    type Item = TokenizerResult<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        let c = match self.source.next()?.into() {
            Ok(v) => v,
            Err(e) => return Some(e),
        };

        match c {
            v @ ('a'..='z' | 'A'..='Z') => Some(consume_ident(v, self.source).map(Token::Ident)),
            _ => todo!(),
        }
    }
}

fn consume_ident<I>(prepend: char, source: &mut I) -> TokenizerResult<String>
where
    I: Iterator<Item = TokenizerResult<char>> + StreamPrepend<TokenizerResult<char>>,
{
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
    }
    StreamResult::Ok(res)
}
