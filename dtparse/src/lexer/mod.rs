use crate::{
    lexer::{
        compiler_directives::{
            CompilerDirective, consume_compiler_directive, consume_compiler_directive_or_root_node,
        },
        expressions::{consume_expression, consume_label_reference},
        node::{consume_node_address, deref_ident_to_node_name},
    },
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        Report,
    },
    result::{ParseErrorReport, StreamResult, StreamedError, Warnings},
    stream_utils::StreamPrepend,
    tokenizer::{
        ArithmeticOperator, Comment, CommentType, GenericLiteral, GenericLiteralType, GroupType,
        LiteralToken, Span, SpanToken, Token, WhitespaceToken, WhitespaceTokenType,
    },
};

pub(crate) mod compiler_directives;
pub(crate) mod expressions;
pub(crate) mod node;

type StreamItem<T> = StreamResult<T, StreamedError<ParseErrorReport>>;
type TokenizerStreamItem = StreamResult<SpanToken, ParseErrorReport>;
type MultiErrorItem<T> = StreamResult<T, StreamedError<Vec<ParseErrorReport>>>;

#[derive(Debug)]
pub enum LexerToken {
    Invalid,
    Newline,
    CompilerDirective(CompilerDirective),
    Label {
        name: (String, Span),
        colon: Span,
    },
    RootNodeStart {
        slash: Span,
        opening_delimiter: Span,
    },
    /// `&node_label {`
    RefNodeStart {
        reference: Reference,
        opening_delimiter: Span,
    },
    NodeStart {
        name: (String, Span),
        unit_address: Option<(String, Span)>,
        opening_delimiter: Span,
    },
    NodeEnd {
        closing_delimiter: Span,
    },
    Statement(Statement),
}

impl LexerToken {
    /// # Panics
    /// Will panic if the token is `Newline` or `Invalid`
    pub fn start_span(&self) -> &Span {
        match self {
            Self::Invalid | Self::Newline => panic!(),
            Self::CompilerDirective(directive) => directive.ident_span(),
            Self::Label { name, .. } => &name.1,
            Self::RootNodeStart { slash, .. } => slash,
            Self::RefNodeStart { reference, .. } => reference.ampersand(),
            Self::NodeStart { name, .. } => &name.1,
            Self::NodeEnd { closing_delimiter } => closing_delimiter,
            Self::Statement(Statement::FlagProperty { property_name }) => &property_name.1,
            Self::Statement(Statement::PropertyAssignment { property_name, .. }) => {
                &property_name.1
            }
        }
    }
}

#[derive(Debug)]
pub enum Statement {
    PropertyAssignment {
        property_name: (String, Span),
        eq: Span,
        expr: Vec<Item>,
    },
    FlagProperty {
        property_name: (String, Span),
    },
}

#[derive(Debug)]
pub enum ArrayItem<T> {
    Newline,
    Item(T),
}

#[derive(Debug, PartialEq)]
pub enum NumericLiteral {
    Decimal((String, Span)),
    Hexadecimal((String, Span)),
}

impl NumericLiteral {
    pub fn span(&self) -> &Span {
        match self {
            Self::Hexadecimal((_, span)) => span,
            Self::Decimal((_, span)) => span,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ArithmeticOperation {
    Addition,
    Subtraction,
    Multiplication,
    Division,
    Modulo,
}

#[derive(Debug, PartialEq)]
pub enum RelationalOperation {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Debug, PartialEq)]
pub enum LogicalOperation {
    And,
    Or,
}

#[derive(Debug, PartialEq)]
pub enum BitwiseOperation {
    And,
    Or,
    Xor,
    LeftShift,
    RightShift,
}

#[derive(Debug, PartialEq)]
pub enum Expression {
    Invalid,
    NumericLiteral(NumericLiteral),
    Group(Box<Expression>),
    ArithmeticOperation {
        left: Box<Expression>,
        right: Box<Expression>,
        operator: ArithmeticOperation,
    },
    RelationalOperation {
        left: Box<Expression>,
        right: Box<Expression>,
        operator: RelationalOperation,
    },
    LogicalOperation {
        left: Box<Expression>,
        right: Box<Expression>,
        operator: LogicalOperation,
    },
    LogicalNot(Box<Expression>),
    BitwiseOperation {
        left: Box<Expression>,
        right: Box<Expression>,
        operator: BitwiseOperation,
    },
    BitwiseNot(Box<Expression>),
    TernaryOperation {
        if_expr: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Box<Expression>,
    },
    Reference(Reference),
}

#[derive(Debug)]
pub enum Item {
    ComplierDirective(CompilerDirective),
    Reference(Reference),
    NumericLiteral(Vec<Expression>),
    ByteString(Vec<(String, Span)>),
    String((String, Span)),
}

type NodePathPortion = ((String, Span), Option<NodeAddress>);
type NodeAddress = (String, Span);

#[derive(Debug, PartialEq, Clone)]
pub enum Reference {
    Label(String, Span, Span),
    NodePath(Vec<NodePathPortion>, Span),
}

impl Reference {
    pub(crate) fn ampersand(&self) -> &Span {
        match self {
            Self::Label(_, _, span) => span,
            Self::NodePath(_, span) => span,
        }
    }
}

pub struct Lexer<'a, I> {
    source: &'a mut I,
}

impl<'a, I> Lexer<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self { source }
    }
}

macro_rules! def_yeet {
    () => {
        def_yeet!(@def_yeet [])
    };

    ([$($t: ident)*]) => {
        def_yeet!(@def_yeet [$($t)*])
    };

    (require next from $source: expr => $mode: ident with message default) => {
        def_yeet!(@def_stream_req_next $source, $mode, def_yeet!(@message default))
    };

    (require next from $source: expr => $mode: ident with message $message: expr) => {
        def_yeet!(@def_stream_req_next $source, $mode, def_yeet!(@message $message))
    };

    (optionally get next from $source: expr => $mode: ident) => {
        def_yeet!(@def_stream_opt_next passthrough, $source, $mode)
    };

    (optionally get next from $source: expr => option_wrapped $mode: ident) => {
        def_yeet!(@def_stream_opt_next option_wrap, $source, $mode)
    };

    (@message default) => { "unexpected end after this".to_string() };
    (@message $custom: literal) => { $custom.to_string() };
    (@message $custom: expr) => { $custom };

    (@def_yeet [$($res_map: tt)*]) => {
        macro_rules! yeet_value {
            ($v: expr) => {
                def_yeet!(@inner_match $v, raw, single, [$($res_map)*])
            };
        }
    };

    (@def_stream_req_next $source: expr, $mode: ident, $message: expr) => {
        macro_rules! req_next {
            ($after: expr) => {
                def_yeet!(@match_option_return $source.next(), $after, $mode, $message)
            };
        }
    };

    (@def_stream_opt_next $option_mode: ident, $source: expr, $mode: ident) => {
        macro_rules! try_next {
            () => {
                def_yeet!(@match_option_optional $source.next(), $mode, [$option_mode])
            };
        }
    };

    (@inner_err raw $mode: ident, $e: expr) => { def_yeet!(@inner_multi_err $mode $e) };
    (@inner_err streamed $mode: ident, $e: expr) => {
        StreamedError::ShouldEnd(def_yeet!(@inner_multi_err $mode $e))
    };
    (@inner_multi_err single $e: expr) => { $e };
    (@inner_multi_err vec $e: expr) => { vec![$e] };

    (@inner_err_end vec $after: expr, $message: expr) => {
        return err!(end_multi [{ UnexpectedEof, [
            ($message, $after)
        ]}])
    };
    (@inner_err_end single $after: expr, $message: expr) => {
        return err!(end [{ UnexpectedEof, [
            ($message, $after.clone())
        ]}])
    };

    (@ret_map $_: ident passthrough $v: expr) => { $v };
    (@ret_map $_: ident option_wrap $v: expr) => { Some($v) };
    (@ret_map io_err vectorize $v: expr) => { $v };
    (@ret_map proc_err vectorize $v: expr) => { $v.map_err(|e| e.map(|e| vec![e])) };
    (@ret_map $err_ty: ident [] $v: expr) => { $v };
    (@ret_map $err_ty: ident [$op: ident $($r: ident)*] $v: expr) => {
        def_yeet!(@ret_map $err_ty $op def_yeet!(@ret_map $err_ty [$($r)*] $v))
    };

    (@match_option_return $from: expr, $after: expr, $mode: ident, $message: expr) => {
        match $from {
            Some(v) => def_yeet!(@inner_match v, streamed, $mode, [passthrough]),
            None => def_yeet!(@inner_err_end $mode $after, $message),
        }
    };

    (@match_option_optional $from: expr, $mode: ident, [$($ret_map: ident)*]) => {
        match $from {
            Some(v) => Some(def_yeet!(@inner_match v, streamed, $mode, [$($ret_map)*])),
            None => None,
        }
    };

    (@inner_match $from: expr, $stream_mode: ident, $mode: ident, [$($ret_map: tt)*]) => {
        match $from {
            StreamResult::Ok(v) => v,
            StreamResult::IoError(e) => return def_yeet!(@ret_map io_err
                [$($ret_map)*] StreamResult::IoError(e)
            ),
            StreamResult::ProcessingError(e) => {
                return def_yeet!(@ret_map proc_err [$($ret_map)*] StreamResult::ProcessingError(
                    def_yeet!(@inner_err $stream_mode $mode, e)
                ))
            }
        }
    };
}

use def_yeet;

#[derive(Debug)]
pub struct LexerItem {
    pub token: LexerToken,
    pub reports: Option<Vec<ParseErrorReport>>,
    pub prevent_compilation: bool,
}

impl<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>> Iterator
    for Lexer<'_, I>
{
    type Item = MultiErrorItem<LexerItem>;

    fn next(&mut self) -> Option<Self::Item> {
        def_yeet!(optionally get next from self.source => option_wrapped vec);
        def_yeet!([option_wrap vectorize]);
        skip_possible(self.source);
        match yeet_value!(opt_consume_any_ident(self.source)) {
            Ok(v) => {
                return Some(consume_statement(self.source, v));
            }
            Err(_) => {}
        }
        let token = try_next!()?;
        match token.token {
            Token::Ampersand => Some(consume_ref_node_start(self.source, token.span)),
            Token::GroupClosing(GroupType::Brace) => {
                skip_possible(self.source);
                let reports = match try_next!() {
                    Some(SpanToken {
                        token: Token::Semicolon,
                        ..
                    }) => None,
                    Some(SpanToken { span, token: t }) => {
                        let res = vec![err!(raw [{ UnexpectedToken, [
                            ("group closing must have a trailing semicolon", token.span.clone()),
                            ("expected a semicolon here", span.clone())
                        ]}])];
                        self.source
                            .push(StreamResult::Ok(SpanToken { span, token: t }));
                        Some(res)
                    }
                    None => Some(vec![err!(raw [{ UnexpectedToken, [
                        ("expected a semicolon after this", token.span.clone()),
                    ]}])]),
                };
                Some(StreamResult::Ok(LexerItem {
                    token: LexerToken::NodeEnd {
                        closing_delimiter: token.span,
                    },
                    prevent_compilation: reports.is_some(),
                    reports,
                }))
            }
            Token::Slash => Some(
                consume_compiler_directive_or_root_node(self.source, token.span)
                    .map_err(|e| e.map(|e| vec![e]))
                    .map(|(token, errors)| match token {
                        Some(v) => LexerItem {
                            token: v,
                            prevent_compilation: !errors.is_empty(),
                            reports: Some(errors),
                        },
                        None => LexerItem {
                            token: LexerToken::Invalid,
                            reports: Some(errors),
                            prevent_compilation: true,
                        },
                    }),
            ),
            _ => Some(StreamResult::Ok(LexerItem {
                token: LexerToken::Invalid,
                reports: Some(vec![err!(raw [{ UnexpectedToken, [
                    ("unexpected token", token.span),
                ]}])]),
                prevent_compilation: true,
            })),
        }
    }
}

fn skip_while<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    skipper: impl Fn(&TokenizerStreamItem) -> bool,
) -> usize {
    let mut skipped = 0;
    while let Some(v) = source.next() {
        if !skipper(&v) {
            source.push(v);
            break;
        } else {
            skipped += 1;
        }
    }
    skipped
}

fn skip_while_no_push<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    skipper: impl Fn(&TokenizerStreamItem) -> bool,
) -> usize {
    let mut skipped = 0;
    while let Some(v) = source.next() {
        if !skipper(&v) {
            break;
        } else {
            skipped += 1;
        }
    }
    skipped
}

fn skip_tokens<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    skipper: impl Fn(&Token) -> bool,
) -> usize {
    skip_while(source, |v| match v {
        StreamResult::Ok(v) => skipper(&v.token),
        _ => false,
    })
}

fn skip_tokens_no_push<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    skipper: impl Fn(&Token) -> bool,
) -> usize {
    let mut skipped = 0;
    while let Some(v) = source.next() {
        match v {
            StreamResult::Ok(v) => {
                if !skipper(&v.token) {
                    break;
                } else {
                    skipped += 1;
                }
            }
            v => {
                source.push(v);
                break;
            }
        }
    }
    skipped
}

fn skip_opt<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    skipper: impl Fn(&TokenizerStreamItem) -> bool,
) -> usize {
    if let Some(v) = source.next() {
        if !skipper(&v) {
            source.push(v);
            0
        } else {
            1
        }
    } else {
        0
    }
}

macro_rules! auto_parser {
    (_skip_wrapper $name: ident, $skipper: expr, $fn: ident) => {
        fn $name<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
            source: &mut I,
        ) -> usize {
            $fn(source, $skipper)
        }
    };

    (_skip_inline_wrapper $source: expr, $skipper: expr, $fn: ident) => {
        $fn($source, $skipper)
    };

    (skip $name: ident, $skipper: expr) => {
        auto_parser!(_skip_wrapper $name, $skipper, skip_while);
    };

    (skip_no_push $name: ident, $skipper: expr) => {
        auto_parser!(_skip_wrapper $name, $skipper, skip_while_no_push);
    };

    (skip_single $name: ident, $skipper: expr) => {
        auto_parser!(_skip_wrapper $name, $skipper, skip_opt);
    };

    (skip_tokens $name: ident, $skipper: expr) => {
        auto_parser!(skip $name, |v| {
            match v {
                StreamResult::Ok(SpanToken { token, .. }) => $skipper(token),
                _ => false,
            }
        });
    };

    (skip_tokens_no_push $name: ident, $skipper: expr) => {
        auto_parser!(_skip_wrapper $name, $skipper, skip_tokens_no_push);
    };

    (skip_token $name: ident, $skipper: expr) => {
        auto_parser!(skip_single $name, |v| {
            match v {
                StreamResult::Ok(SpanToken { token, .. }) => $skipper(token),
                _ => false,
            }
        })
    };

    (skip_token_inline $source: expr, $skipper: expr) => {
        auto_parser!(_skip_inline_wrapper $source, |v| {
            match v {
                StreamResult::Ok(SpanToken { token, .. }) => $skipper(token),
                _ => false,
            }
        }, skip_opt)
    };
}

use auto_parser;

auto_parser!(skip_tokens skip_block_comments, |v| {
    matches!(v, &Token::Comment(Comment { of_type: CommentType::Block, .. }))
});

auto_parser!(skip_tokens skip_inline_whitespace, |v| {
    matches!(v, &Token::Whitespace(
        WhitespaceToken { of_type: WhitespaceTokenType::Tab | WhitespaceTokenType::Space, .. }
    ))
});

auto_parser!(skip_tokens skip_whitespace, |v| {
    matches!(v, &Token::Whitespace(_))
});

auto_parser!(skip_tokens skip_possible_inline, |v: &Token| {
    match *v {
        Token::Whitespace(WhitespaceToken { of_type: WhitespaceTokenType::Newline, .. }) => false,
        Token::Comment(Comment { of_type: CommentType::Line, .. }) => false,
        Token::Whitespace(_) | Token::Comment(_) => true,
        _ => false,
    }
});

auto_parser!(skip_tokens skip_possible, |v| {
    matches!(v, &(Token::Whitespace(_) | Token::Comment(_)))
});

auto_parser!(skip_tokens_no_push skip_to_statement_end, |t| {
    matches!(t, &Token::Semicolon)
});

enum ExtendedIdent {
    Label(String),
    Node(String),
    Attribute(String),
}

impl ExtendedIdent {
    /// # Error
    /// Returns the indices of the invalid symbols and the inner content
    fn req_label(self) -> Result<String, (Vec<usize>, String)> {
        match self {
            Self::Label(v) => Ok(v),
            Self::Attribute(v) | Self::Node(v) => {
                let mut idcs = Vec::new();
                for (idx, ch) in v.chars().enumerate() {
                    match ch {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {}
                        _ => idcs.push(idx),
                    }
                }
                Err((idcs, v))
            }
        }
    }

    /// # Error
    /// Returns the indices of the invalid symbols and the inner content
    fn req_node_name(self) -> Result<String, (Vec<usize>, String)> {
        match self {
            Self::Label(v) | Self::Node(v) => Ok(v),
            Self::Attribute(v) => {
                let mut idcs = Vec::new();
                for (idx, ch) in v.chars().enumerate() {
                    match ch {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '+' | '-' | '.' | ',' => {}
                        _ => idcs.push(idx),
                    }
                }
                Err((idcs, v))
            }
        }
    }

    fn req_attribute_name(self) -> String {
        match self {
            Self::Label(v) | Self::Node(v) | Self::Attribute(v) => v,
        }
    }
}

fn opt_consume_any_ident<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<Result<(ExtendedIdent, Span), Option<Span>>> {
    let mut res = String::new();
    #[derive(Clone, Copy)]
    enum ExtendedIdentType {
        Label,
        Node,
        Attribute,
    }
    impl ExtendedIdentType {
        fn incr(self, token: &Token) -> Option<Self> {
            match (self, token) {
                (v, Token::Literal(LiteralToken::Ident(_))) => Some(v),
                (
                    Self::Label,
                    Token::Comma
                    | Token::Period
                    | Token::ArithmeticOperator(ArithmeticOperator::Plus | ArithmeticOperator::Dash),
                ) => Some(Self::Node),
                (
                    v @ (Self::Node | Self::Attribute),
                    Token::Comma
                    | Token::Period
                    | Token::ArithmeticOperator(ArithmeticOperator::Plus | ArithmeticOperator::Dash),
                ) => Some(v),
                (_, Token::Hash | Token::QuestionMark) => Some(Self::Attribute),
                (_, _) => None,
            }
        }
    }
    let mut of_type = ExtendedIdentType::Label;

    let mut ptr = None;
    let mut span = 0;
    loop {
        let l = match source.next() {
            Some(StreamResult::Ok(v)) => v,
            Some(v) => {
                source.push(v);
                break;
            }
            None => break,
        };
        let new_type = match of_type.incr(&l.token) {
            Some(v) => v,
            None => {
                let span = l.span.clone();
                source.push(StreamResult::Ok(l));
                if ptr.is_none() {
                    return StreamResult::Ok(Err(Some(span)));
                } else {
                    break;
                }
            }
        };
        if ptr.is_none() {
            ptr = Some(l.span.ptr);
        }
        res.push_str(&String::from(l.token));
        span += l.span.span;
        of_type = new_type;
    }

    let Some(ptr) = ptr else {
        return StreamResult::Ok(Err(None));
    };
    let span = Span { ptr, span };
    StreamResult::Ok(Ok((
        match of_type {
            ExtendedIdentType::Label => ExtendedIdent::Label(res),
            ExtendedIdentType::Node => ExtendedIdent::Node(res),
            ExtendedIdentType::Attribute => ExtendedIdent::Attribute(res),
        },
        span,
    )))
}

fn consume_any_ident<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<(ExtendedIdent, Span)> {
    opt_consume_any_ident(source).map(|v| v.unwrap())
}

macro_rules! warning {
    (@msg $message: literal) => {
        $message.to_string()
    };

    (@msg $message: expr) => {
        $message
    };

    (@message ($message: expr, $span: expr, $ptr: expr)) => {
        Box::new(PrimitiveReportMessage::warning(warning!(@msg $message), $span, $ptr))
            as Box<dyn ReportInlineMessage>
    };

    (@message ($message: expr, $span: expr)) => {
        Box::new(
            crate::report::PrimitiveReportMessage::warning(
                warning!(@msg $message), $span.span, $span.ptr
            )
        ) as Box<dyn crate::report::ReportInlineMessage>
    };

    (@messages [$($message: tt),*]) => {
        vec![$(warning!(@message $message)),*]
    };

    (@segment $warning_type: ident, [$($message: tt),*]) => {
        Box::new(crate::report::PrimitiveReportSegment::new(
            Some(crate::report::PrimitiveMainMessage::warning(
                crate::errors::Warnings::$warning_type.message(),
                crate::errors::Warnings::$warning_type.id(),
            )),
            warning!(@messages [$($message),*]),
        ))
    };

    (@report $({ $warning_type: ident, [$($message: tt),*$(,)?] }),*$(,)?) => {
        Box::new(crate::report::PrimitiveReport::new(
            vec![$(warning!(@segment $warning_type, [$($message),*])),*]
        )) as crate::result::ParseErrorReport
    };

    ($($t: tt)*) => {
        warning!(@report $($t)*)
    };
}

macro_rules! err {
    (@msg $message: literal) => {
        $message.to_string()
    };

    (@msg $message: expr) => {
        $message
    };

    (@err_ty $error_type: ident) => {
        Errors::$error_type
    };

    (@message ($message: expr, $span: expr, $ptr: expr)) => {
        Box::new(crate::report::PrimitiveReportMessage::error(err!(@msg $message), $span, $ptr))
            as Box<dyn crate::report::ReportInlineMessage>
    };

    (@message ($message: expr, $span: expr)) => {
        Box::new(crate::report::PrimitiveReportMessage::error(
                err!(@msg $message), $span.span, $span.ptr)
        ) as Box<dyn crate::report::ReportInlineMessage>
    };

    (@messages [$($message: tt),*]) => {
        vec![$(err!(@message $message)),*]
    };

    (@segment $error_type: ident, [$($message: tt),*]) => {
        Box::new(crate::report::PrimitiveReportSegment::new(
            Some(crate::report::PrimitiveMainMessage::error(
                crate::errors::Errors::$error_type.message(),
                crate::errors::Errors::$error_type.id(),
            )),
            err!(@messages [$($message),*]),
        ))
    };

    (@report $({ $error_type: ident, [$($message: tt),*$(,)?] }),*$(,)?) => {
        Box::new(crate::report::PrimitiveReport::new(
            vec![$(err!(@segment $error_type, [$($message),*])),*]
        )) as crate::result::ParseErrorReport
    };

    (cont [$($t: tt)*]) => {
        crate::result::StreamResult::ProcessingError(
            crate::result::StreamedError::CanContinue(err!(@report $($t)*))
        )
    };

    (cont_multi [$($t: tt)*]) => {
        crate::result::StreamResult::ProcessingError(
            crate::result::StreamedError::CanContinue(vec![err!(@report $($t)*)])
        )
    };

    (end [$($t: tt)*]) => {
        crate::result::StreamResult::ProcessingError(
            crate::result::StreamedError::ShouldEnd(err!(@report $($t)*))
        )
    };

    (end_multi [$($t: tt)*]) => {
        crate::result::StreamResult::ProcessingError(
            crate::result::StreamedError::ShouldEnd(vec![err!(@report $($t)*)])
            )
    };

    (raw [$($t: tt)*]) => {
        err!(@report $($t)*)
    };
}

pub(crate) use err;
pub(crate) use warning;

enum StatementStart {
    Label {
        name: String,
        span: Span,
    },
    Attribute {
        name: String,
        span: Span,
    },
    Node {
        name: String,
        address: Option<u64>,
        span: Span,
    },
}

fn req_token_after<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    matcher_continue: impl Fn(&Token) -> bool,
    matcher_end: impl Fn(&Token) -> bool,
    err_msg_eof: &'static str,
    err_after: &Span,
    err_msg_after: &'static str,
    err_msg_unexpected: &'static str,
) -> StreamItem<SpanToken> {
    def_yeet!(require next from source => single with message err_msg_eof.to_string());
    loop {
        let next = req_next!(err_after.clone());
        if !matcher_continue(&next.token) {
            if matcher_end(&next.token) {
                break StreamResult::Ok(next);
            } else {
                return err!(cont [{ UnexpectedToken, [
                    (err_msg_after.to_string(), err_after.clone()),
                    (err_msg_unexpected.to_string(), next.span),
                ]}]);
            }
        }
    }
}

fn try_token_after<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    matcher_continue: impl Fn(&Token) -> bool,
    matcher_end: impl Fn(&Token) -> bool,
    err_msg_eof: &'static str,
    err_after: &Span,
    err_msg_after: &'static str,
    err_msg_unexpected: &'static str,
) -> StreamItem<(Option<SpanToken>, Option<ParseErrorReport>)> {
    def_yeet!(require next from source => single with message err_msg_eof.to_string());
    let mut error = None;
    loop {
        let next = req_next!(err_after.clone());
        if !matcher_continue(&next.token) {
            if matcher_end(&next.token) {
                return StreamResult::Ok((Some(next), error));
            } else {
                if error.is_some() {
                    break;
                }
                error = Some(err!(raw [{ UnexpectedToken, [
                    (err_msg_after.to_string(), err_after.clone()),
                    (err_msg_unexpected.to_string(), next.span),
                ]}]));
            }
        }
    }
    StreamResult::Ok((None, error))
}

fn consume_statement<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    start: (ExtendedIdent, Span),
) -> MultiErrorItem<LexerItem> {
    def_yeet!(require next from source => vec with message default);
    def_yeet!([vectorize]);
    let skipped = skip_possible(source);
    let next = req_next!(start.1);
    match next.token {
        Token::At => {
            let to_push = if skipped > 0 {
                Some(err!(raw [{ UnexpectedWhitespace, [
                    ("a whitespace is unexpected after this node name", start.1.clone()),
                    ("and this node address separator", next.span.clone()),
                ]}]))
            } else {
                None
            };
            let (node_name, mut errors) = deref_ident_to_node_name((start.0, &start.1), true);
            if let Some(to_push) = to_push {
                errors.push(to_push);
            }
            let (addr, e) = match yeet_value!(consume_node_address(source)) {
                Ok(v) => v,
                Err(Some(v)) => {
                    return StreamResult::Ok(LexerItem {
                        token: LexerToken::Invalid,
                        reports: Some(vec![err!(raw [{ UnexpectedToken, [
                            ("this is not a valid node address", v),
                        ]}])]),
                        prevent_compilation: true,
                    });
                }
                Err(None) => {
                    return StreamResult::Ok(LexerItem {
                        token: LexerToken::Invalid,
                        reports: Some(vec![err!(raw [{ UnexpectedToken, [
                            ("expected a node address after this, encountered eof", next.span),
                        ]}])]),
                        prevent_compilation: true,
                    });
                }
            };
            errors.extend(e);
            skip_possible(source);
            let opening = yeet_value!(req_token_after(
                source,
                |t| { matches!(t, Token::Whitespace(_) | Token::Comment(_)) },
                |t| { matches!(t, Token::GroupOpening(GroupType::Brace)) },
                "expected a `{` somewhere after this node address, got eof",
                &addr.1,
                "after this node address",
                "unexpected token, expected `{`"
            ));
            StreamResult::Ok(LexerItem {
                token: LexerToken::NodeStart {
                    name: (node_name, start.1),
                    unit_address: Some(addr),
                    opening_delimiter: opening.span,
                },
                prevent_compilation: !errors.is_empty(),
                reports: Some(errors),
            })
        }
        Token::Colon => {
            let (name, mut reports, abort) = deref_label((start.0, &start.1));
            if skipped > 0 {
                let mut reports_inner = reports.unwrap_or_default();
                reports_inner.push(err!(raw [{ UnexpectedWhitespace, [
                    (
                        "a whitespace or a comment is unexpected after this label name",
                        start.1.clone()
                    ),
                    ("and before this colon", next.span.clone()),
                ]}]));
                reports = Some(reports_inner);
            }
            StreamResult::Ok(LexerItem {
                token: LexerToken::Label {
                    name: (name, start.1),
                    colon: next.span,
                },
                prevent_compilation: abort,
                reports,
            })
        }
        Token::GroupOpening(GroupType::Brace) => {
            let (name, reports) = deref_ident_to_node_name((start.0, &start.1), true);
            StreamResult::Ok(LexerItem {
                token: LexerToken::NodeStart {
                    name: (name, start.1),
                    unit_address: None,
                    opening_delimiter: next.span,
                },
                prevent_compilation: !reports.is_empty(),
                reports: Some(reports),
            })
        }
        Token::Equal => {
            let (property_name, reports, abort) = deref_attribute_name((start.0, &start.1), false);
            let (r, w, e) = match consume_attribute_value(source, &next.span) {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
            };
            let abort = abort || (e.is_empty());
            let mut reports = reports.unwrap_or_default();
            reports.extend(w.into_iter());
            reports.extend(e.into_iter());
            StreamResult::Ok(LexerItem {
                token: LexerToken::Statement(Statement::PropertyAssignment {
                    property_name: (property_name, start.1),
                    eq: next.span,
                    expr: r,
                }),
                reports: if reports.is_empty() {
                    None
                } else {
                    Some(reports)
                },
                prevent_compilation: abort,
            })
        }
        Token::Semicolon => {
            let (property_name, reports, abort) = deref_attribute_name((start.0, &start.1), false);
            StreamResult::Ok(LexerItem {
                token: LexerToken::Statement(Statement::FlagProperty {
                    property_name: (property_name, start.1),
                }),
                reports,
                prevent_compilation: abort,
            })
        }
        _ => {
            let span = next.span.clone();
            source.push(StreamResult::Ok(next));
            err!(cont_multi [{ UnexpectedToken, [
                ("this is a candidate for a statement start, a node/attribute name", start.1),
                ("however this is an unexpected continuation in both cases", span)
            ] }])
        }
    }
}

fn consume_ref_node_start<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    ampersand: Span,
) -> MultiErrorItem<LexerItem> {
    let mut errors = Vec::new();
    let reference = match consume_label_reference(source, ampersand.clone()) {
        StreamResult::Ok(v) => Some(v),
        StreamResult::IoError(e) => return StreamResult::IoError(e),
        StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
            errors = e;
            None
        }
        StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
    };
    def_yeet!();
    let (opening, e) = match try_token_after(
        source,
        |t| matches!(t, Token::Whitespace(_) | Token::Comment(_)),
        |t| matches!(t, Token::GroupOpening(GroupType::Brace)),
        "this label's path remains unclosed",
        &ampersand,
        "after this label",
        "expected a node opening",
    ) {
        StreamResult::Ok(v) => v,
        StreamResult::IoError(e) => return StreamResult::IoError(e),
        StreamResult::ProcessingError(StreamedError::ShouldEnd(e)) => {
            errors.push(e);
            return StreamResult::ProcessingError(StreamedError::ShouldEnd(errors));
        }
        StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
            errors.push(e);
            return StreamResult::ProcessingError(StreamedError::CanContinue(errors));
        }
    };
    if let Some(e) = e {
        errors.push(e);
    }
    StreamResult::Ok(LexerItem {
        token: match (reference, opening) {
            (Some(reference), Some(opening)) => LexerToken::RefNodeStart {
                reference,
                opening_delimiter: opening.span,
            },
            _ => LexerToken::Invalid,
        },
        prevent_compilation: !errors.is_empty(),
        reports: Some(errors),
    })
}

/// # Returns
/// (value, errors, should_end)
fn deref_label(name: (ExtendedIdent, &Span)) -> (String, Option<Vec<ParseErrorReport>>, bool) {
    let mut errors = Vec::new();
    let label_name = match name.0.req_label() {
        Ok(v) => v,
        Err((e, v)) => {
            let symbol = e.first().unwrap();
            let ptr = name.1.ptr.clone().offset(symbol);
            errors.push(err!(raw [{ InvalidLabelName, [
                ("invalid symbol in a label name", 1, ptr),
            ]}]));
            v
        }
    };
    let abort = !errors.is_empty();
    (
        label_name,
        if !errors.is_empty() {
            Some(errors)
        } else {
            None
        },
        abort,
    )
}

/// # Returns
/// (value, errors, should_end)
fn deref_attribute_name(
    name: (ExtendedIdent, &Span),
    disable_opinions: bool,
) -> (String, Option<Vec<ParseErrorReport>>, bool) {
    let mut errors = Vec::new();
    let attr_name = name.0.req_attribute_name();
    if !disable_opinions {
        match attr_name.chars().next().unwrap() {
            'a'..='z' | 'A'..='Z' | '#' => {}
            _ => errors.push(
                Box::new(PrimitiveReport::single(PrimitiveReportSegment::single(
                    PrimitiveMainMessage::warning(
                        Warnings::WeirdPropertyName.message(),
                        Warnings::WeirdPropertyName.id(),
                    ),
                    PrimitiveReportMessage::warning(
                        "attribute names should begin with a letter or a hashtag".to_string(),
                        1,
                        name.1.ptr.clone(),
                    ),
                ))) as Box<dyn Report>,
            ),
        }
    }
    (
        attr_name,
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        },
        false,
    )
}

type WarningReports = Vec<ParseErrorReport>;
type ErrorReports = Vec<ParseErrorReport>;

fn consume_attribute_value<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    eq: &Span,
) -> MultiErrorItem<(Vec<Item>, WarningReports, ErrorReports)> {
    def_yeet!(require next from source => vec with message
        "this value assignment does not end until eof, perhaps you forgot a semicolon?"
    );
    auto_parser!(skip_tokens skip_to_expr_delim, |v| {
        !matches!(v, &(Token::Comma | Token::Semicolon))
    });
    let mut res = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    macro_rules! yield_consumer {
        (with_warnings $call: expr) => {{
            let (r, w, e) = match $call {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
            };
            warnings.extend(w.into_iter());
            errors.extend(e.into_iter());
            r
        }};

        ($call: expr) => {{
            let (r, e) = match $call {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
            };
            errors.extend(e.into_iter());
            r
        }};
    }
    let mut expecting_item = true;
    loop {
        skip_possible(source);
        let token = req_next!(eq.clone());
        match (token.token, expecting_item) {
            (Token::Whitespace(_) | Token::Comment(_), _) => unreachable!(),
            (Token::Comma, false) => {
                expecting_item = true;
            }
            (Token::Comma, true) => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected an item before this (multiple commas)", token.span),
                ]}]));
            }
            (Token::Lt, true) => {
                expecting_item = false;
                let r = yield_consumer!(with_warnings consume_integer(source, token.span));
                res.push(r);
            }
            (Token::GroupOpening(GroupType::Square), true) => {
                expecting_item = false;
                let r = yield_consumer!(consume_byte_string(source, token.span));
                res.push(r);
            }
            (Token::Lt | Token::GroupOpening(GroupType::Square), false) => {
                skip_to_expr_delim(source);
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("did not expect this, perhaps you forgot a comma before this?", token.span),
                ]}]));
            }
            (Token::Literal(LiteralToken::String(v)), true) => {
                expecting_item = false;
                res.push(Item::String((v, token.span)));
            }
            (Token::Literal(LiteralToken::String(_)), false) => {
                skip_to_expr_delim(source);
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("assignment started here", eq.clone()),
                    ("did not expect this, perhaps you forgot a semicolon before this?", 1, token.span.ptr),
                ]}]));
            }
            (Token::Slash, true) => {
                let directive = yield_consumer!(
                    consume_compiler_directive(source, token.span).map_err(|e| e.map(|e| vec![e]))
                );
                match directive {
                    Some(v) => res.push(Item::ComplierDirective(v)),
                    None => {}
                }
            }
            (Token::Ampersand, true) => {
                res.push(match consume_label_reference(source, token.span) {
                    StreamResult::Ok(v) => Item::Reference(v),
                    StreamResult::IoError(e) => return StreamResult::IoError(e),
                    StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
                        errors.extend(e);
                        continue;
                    }
                    StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
                });
                expecting_item = false;
            }
            (Token::Semicolon, false) => {
                break;
            }
            (Token::Semicolon, true) => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("did not expect an end just yet, expected an item here", token.span),
                ]}]));
                break;
            }
            (_, false) => {
                skip_to_expr_delim(source);
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("assignment started here", eq.clone()),
                    ("did not expect this, perhaps you forgot a semicolon before this?", token.span),
                ]}]));
            }
            (_, true) => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected an item (starting with one of `[`, `<`, `\"`) here", token.span),
                ]}]));
            }
        }
    }
    StreamResult::Ok((res, warnings, errors))
}

fn consume_integer<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    lt: Span,
) -> MultiErrorItem<(Item, WarningReports, ErrorReports)> {
    def_yeet!(require next from source => vec with message "this remains unclosed until eof");
    let mut res = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    loop {
        skip_possible(source);
        let token = req_next!(lt);
        match token.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                of_type: GenericLiteralType::DecimalNumeric,
                content,
            })) => {
                res.push(Expression::NumericLiteral(NumericLiteral::Decimal((
                    content, token.span,
                ))));
            }
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                of_type: GenericLiteralType::HexadecimalNumeric { prefix },
                mut content,
            })) => {
                if !prefix {
                    errors.push(err!(raw [{ InvalidNumericLiteral, [
                        (
                            "hexadecimals in integer arrays (`<`, `>`) must contain prefix `0x`",
                            token.span.clone()
                        ),
                    ]}]));
                } else {
                    content = content.strip_prefix("0x").unwrap().to_string();
                }
                res.push(Expression::NumericLiteral(NumericLiteral::Hexadecimal((
                    content, token.span,
                ))));
            }
            Token::GroupOpening(GroupType::Paren) => {
                let (r, w, e) = match consume_expression(source, token.span) {
                    StreamResult::Ok(v) => v,
                    StreamResult::IoError(e) => return StreamResult::IoError(e),
                    StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
                };
                warnings.extend(w.into_iter());
                errors.extend(e.into_iter());
                res.push(r);
            }
            Token::Literal(LiteralToken::String(_)) => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected a valid expression (without quotes)", 1, token.span.ptr),
                ]}]));
            }
            Token::Ampersand => {
                res.push(match consume_label_reference(source, token.span) {
                    StreamResult::Ok(v) => Expression::Reference(v),
                    StreamResult::IoError(e) => return StreamResult::IoError(e),
                    StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
                        errors.extend(e);
                        continue;
                    }
                    StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
                });
            }
            Token::Comma => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("integer arrays are space-separated", token.span),
                ]}]));
            }
            Token::Gt => break,
            Token::Whitespace(_) | Token::Comment(_) => unreachable!(),
            _ => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected a valid expression", token.span),
                ]}]));
            }
        }
    }
    StreamResult::Ok((Item::NumericLiteral(res), warnings, errors))
}

fn consume_byte_string<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    opening_bracket: Span,
) -> MultiErrorItem<(Item, ErrorReports)> {
    def_yeet!(require next from source => vec with message "this remains unclosed until eof");
    let mut res = Vec::new();
    let mut errors = Vec::new();
    loop {
        skip_possible(source);
        let token = req_next!(opening_bracket);
        match token.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                of_type: GenericLiteralType::DecimalNumeric,
                content,
            })) => {
                res.push((content, token.span));
            }
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                of_type: GenericLiteralType::HexadecimalNumeric { prefix },
                mut content,
            })) => {
                if prefix {
                    errors.push(err!(raw [{ InvalidNumericLiteral, [
                        (
                            "bytestrings must contain hexadecimals without prefix",
                            2, token.span.ptr.clone()
                        ),
                    ]}]));
                    content = content.strip_prefix("0x").unwrap().to_string();
                }
                res.push((content, token.span));
            }
            Token::Literal(LiteralToken::String(_)) => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected a valid hexadecimal literal (without quotes)", 1, token.span.ptr),
                ]}]));
            }
            Token::Comma => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("bytestrings are space-separated", token.span),
                ]}]));
            }
            Token::GroupClosing(GroupType::Square) => break,
            Token::Whitespace(_) | Token::Comment(_) => unreachable!(),
            _ => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("expected a valid hexadecimal literal", token.span),
                ]}]));
            }
        }
    }
    StreamResult::Ok((Item::ByteString(res), errors))
}

pub struct LexerSkipper<'a, I> {
    source: &'a mut I,
}
