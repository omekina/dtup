use crate::{
    lexer::{
        compiler_directives::{
            CompilerDirective, consume_compiler_directive, consume_compiler_directive_or_root_node,
        },
        expressions::{consume_expression, consume_label_reference},
    },
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        Report, ReportInlineMessage,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError, Warnings},
    stream_utils::StreamPrepend,
    tokenizer::{
        ArithmeticOperator, Comment, CommentType, GenericLiteral, GenericLiteralType, GroupType,
        LiteralToken, Span, SpanToken, Token, WhitespaceToken, WhitespaceTokenType,
    },
};

mod compiler_directives;
mod expressions;

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
    NodeStart {
        name: (String, Span),
        unit_address: Option<(String, Span)>,
        opening_delimiter: Span,
    },
    NodeEnd {
        closing_delimiter: Span,
    },
    Statement(Statement),
    StatementEnd {
        span: Span,
    },
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
    Decimal(String),
    Hexadecimal(String),
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
    NumericLiteral((NumericLiteral, Span)),
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

type NodePathPortion = (String, Span);
type NodeAddress = String;

#[derive(Debug, PartialEq)]
pub enum Reference {
    Label(String, Span),
    NodePath(Vec<NodePathPortion>, Option<(NodeAddress, Span)>),
}

pub struct Lexer<'a, I> {
    source: &'a mut I,
}

impl<'a, I> Lexer<'a, I> {
    pub fn new(source: &'a mut I) -> Self {
        Self { source }
    }
}

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
        macro_rules! try_yield {
            (option) => {
                match self.source.next() {
                    Some(StreamResult::Ok(v)) => Some(v),
                    Some(StreamResult::IoError(e)) => return Some(StreamResult::IoError(e)),
                    Some(StreamResult::ProcessingError(e)) => {
                        return Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(
                            vec![e],
                        )));
                    }
                    None => None,
                }
            };

            () => {
                try_yield!(option)?
            };
        }
        skip_possible(self.source);
        let token = try_yield!();
        match token.token {
            Token::Literal(LiteralToken::Ident(_)) => {
                self.source.push(StreamResult::Ok(token));
                Some(consume_statement(self.source))
            }
            Token::GroupClosing(GroupType::Brace) => {
                skip_possible(self.source);
                let reports = match try_yield!(option) {
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
            Token::Whitespace(_) | Token::Comment(_) => unreachable!(),
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

pub(self) fn skip_while<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
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

pub(self) fn skip_tokens<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    skipper: impl Fn(&Token) -> bool,
) -> usize {
    skip_while(source, |v| match v {
        StreamResult::Ok(v) => skipper(&v.token),
        _ => false,
    })
}

pub(self) fn skip_opt<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
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

pub(self) use auto_parser;

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
) -> Option<(ExtendedIdent, Span)> {
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
                (v @ _, Token::Literal(LiteralToken::Ident(_))) => Some(v),
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
                source.push(StreamResult::Ok(l));
                break;
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
        return None;
    };
    let span = Span { ptr, span };
    Some((
        match of_type {
            ExtendedIdentType::Label => ExtendedIdent::Label(res),
            ExtendedIdentType::Node => ExtendedIdent::Node(res),
            ExtendedIdentType::Attribute => ExtendedIdent::Attribute(res),
        },
        span,
    ))
}

fn consume_any_ident<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> (ExtendedIdent, Span) {
    opt_consume_any_ident(source).unwrap()
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
        Box::new(PrimitiveReportMessage::warning(warning!(@msg $message), $span.span, $span.ptr))
            as Box<dyn ReportInlineMessage>
    };

    (@messages [$($message: tt),*]) => {
        vec![$(warning!(@message $message)),*]
    };

    (@segment $warning_type: ident, [$($message: tt),*]) => {
        Box::new(PrimitiveReportSegment::new(
            Some(PrimitiveMainMessage::warning(
                Warnings::$warning_type.message(), Warnings::$warning_type.id(),
            )),
            warning!(@messages [$($message),*]),
        ))
    };

    (@report $({ $warning_type: ident, [$($message: tt),*$(,)?] }),*$(,)?) => {
        Box::new(PrimitiveReport::new(
            vec![$(warning!(@segment $warning_type, [$($message),*])),*]
        )) as ParseErrorReport
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
        Box::new(PrimitiveReportMessage::error(err!(@msg $message), $span, $ptr))
            as Box<dyn ReportInlineMessage>
    };

    (@message ($message: expr, $span: expr)) => {
        Box::new(PrimitiveReportMessage::error(err!(@msg $message), $span.span, $span.ptr))
            as Box<dyn ReportInlineMessage>
    };

    (@messages [$($message: tt),*]) => {
        vec![$(err!(@message $message)),*]
    };

    (@segment $error_type: ident, [$($message: tt),*]) => {
        Box::new(PrimitiveReportSegment::new(
            Some(PrimitiveMainMessage::error(
                Errors::$error_type.message(), Errors::$error_type.id(),
            )),
            err!(@messages [$($message),*]),
        ))
    };

    (@report $({ $error_type: ident, [$($message: tt),*$(,)?] }),*$(,)?) => {
        Box::new(PrimitiveReport::new(
            vec![$(err!(@segment $error_type, [$($message),*])),*]
        )) as ParseErrorReport
    };

    (cont [$($t: tt)*]) => {
        StreamResult::ProcessingError(StreamedError::CanContinue(err!(@report $($t)*)))
    };

    (cont_multi [$($t: tt)*]) => {
        StreamResult::ProcessingError(StreamedError::CanContinue(vec![err!(@report $($t)*)]))
    };

    (end [$($t: tt)*]) => {
        StreamResult::ProcessingError(StreamedError::ShouldEnd(err!(@report $($t)*)))
    };

    (end_multi [$($t: tt)*]) => {
        StreamResult::ProcessingError(StreamedError::ShouldEnd(vec![err!(@report $($t)*)]))
    };

    (raw [$($t: tt)*]) => {
        err!(@report $($t)*)
    };
}

pub(self) use err;
pub(self) use warning;

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

macro_rules! def_try_yield {
    (@default_err) => { "unexpected end after this".to_string() };

    (single_err $source: expr) => {
        def_try_yield!(@def $source, single, def_try_yield!(@default_err))
    };
    (multi_err $source: expr) => {
        def_try_yield!(@def $source, multi, def_try_yield!(@default_err))
    };
    (custom_single_err $source: expr, $message: expr) => {
        def_try_yield!(@def $source, single, $message.to_string())
    };
    (custom_multi_err $source: expr, $message: expr) => {
        def_try_yield!(@def $source, multi, $message.to_string())
    };

    (errs_only $mode: ident $source: expr) => {
        def_try_yield!(@def_errs_only $source, $mode)
    };

    (@def $source: expr, $mode: ident, $message: expr) => {
        macro_rules! try_yield {
            ($after: expr) => {
                def_try_yield!(@inner_match $source, $after, $mode, $message)
            };
        }
    };

    (@def_errs_only $source: expr, $mode: ident) => {
        macro_rules! try_yield_errs {
            () => {
                def_try_yield!(@inner_match_errs_only $source, $mode)
            };
        }
    };

    (@inner_err single $e: expr) => { $e };
    (@inner_err multi $e: expr) => { vec![$e] };

    (@inner_err_end multi $after: expr, $message: expr) => {
        return err!(end_multi [{ UnexpectedEof, [
            ($message, $after)
        ]}])
    };
    (@inner_err_end single $after: expr, $message: expr) => {
        return err!(end [{ UnexpectedEof, [
            ($message, $after.clone())
        ]}])
    };

    (@inner_match $source: expr, $after: expr, $mode: ident, $message: expr) => {
        match $source.next() {
            Some(StreamResult::Ok(v)) => v,
            Some(StreamResult::IoError(e)) => return StreamResult::IoError(e),
            Some(StreamResult::ProcessingError(e)) => {
                (return StreamResult::ProcessingError(StreamedError::ShouldEnd(
                    def_try_yield!(@inner_err $mode e)
                )))
            }
            None => def_try_yield!(@inner_err_end $mode $after, $message),
        }
    };

    (@inner_match_errs_only $source: expr, $mode: ident) => {
        match $source.next() {
            Some(StreamResult::Ok(v)) => Some(v),
            Some(StreamResult::IoError(e)) => return StreamResult::IoError(e),
            Some(StreamResult::ProcessingError(e)) => {
                return StreamResult::ProcessingError(StreamedError::ShouldEnd(
                    def_try_yield!(@inner_err $mode e)
                ))
            }
            None => None,
        }
    };
}

pub(self) use def_try_yield;

fn consume_statement<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> MultiErrorItem<LexerItem> {
    def_try_yield!(multi_err source);
    let start = consume_any_ident(source);
    let skipped = skip_possible(source);
    let next = try_yield!(start.1);
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
            consume_node_address(source, start, next)
                .map(|mut item| {
                    if let Some(to_push) = to_push {
                        let mut report = item.reports.unwrap_or_default();
                        report.push(to_push);
                        item.reports = Some(report);
                    }
                    item
                })
                .map_err(|e| e.map(|e| vec![e]))
        }
        Token::Colon => {
            let (name, mut reports, abort) = deref_label((start.0, &start.1));
            if skipped > 0 {
                let mut reports_inner = reports.unwrap_or_default();
                reports_inner.push(err!(raw [{ UnexpectedWhitespace, [
                    ("a whitespace is unexpected after this label name", start.1.clone()),
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
            let (name, reports) = deref_node_name((start.0, &start.1), true);
            StreamResult::Ok(LexerItem {
                token: LexerToken::NodeStart {
                    name: (name, start.1),
                    unit_address: None,
                    opening_delimiter: next.span,
                },
                prevent_compilation: reports.is_some(),
                reports,
            })
        }
        Token::Equal => {
            let (property_name, reports, abort) = deref_attribute_name((start.0, &start.1));
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
            let (property_name, reports, abort) = deref_attribute_name((start.0, &start.1));
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
                ("expected a valid statement continuation after this", start.1),
                ("unexpected token here", span)
            ] }])
        }
    }
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
) -> (String, Option<Vec<ParseErrorReport>>, bool) {
    let mut errors = Vec::new();
    let attr_name = name.0.req_attribute_name();
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

fn deref_node_name(
    name: (ExtendedIdent, &Span),
    require_letter_start: bool,
) -> (String, Option<Vec<ParseErrorReport>>) {
    let mut errors = Vec::new();
    let node_name = match name.0.req_node_name() {
        Ok(v) => v,
        Err((e, v)) => {
            let symbol = e.first().unwrap();
            let ptr = name.1.ptr.clone().offset(symbol);
            errors.push(err!(raw [{ InvalidNodeName, [
                ("invalid symbol in a node name", 1, ptr)
            ]}]));
            v
        }
    };
    if require_letter_start {
        match node_name.chars().next().unwrap() {
            'a'..='z' | 'A'..='Z' => {}
            _ => errors.push(err!(raw [{ InvalidNodeName, [
                ("node names must start with a letter", 1, name.1.ptr.clone()),
            ]}])),
        }
    }
    (
        node_name,
        if !errors.is_empty() {
            Some(errors)
        } else {
            None
        },
    )
}

fn consume_node_address<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    name: (ExtendedIdent, Span),
    at: SpanToken,
) -> StreamItem<LexerItem> {
    def_try_yield!(single_err source);
    let mut errors = Vec::new();
    skip_block_comments(source);
    let ident = opt_consume_any_ident(source)
        .map(|(ident, span)| (deref_node_name((ident, &span), false), span));
    let (address, address_span) = match ident {
        Some(((ident, errors_inner), span)) => {
            errors.extend(errors_inner.unwrap_or_default());
            (ident, span)
        }
        None => {
            return err!(cont [{ UnexpectedToken, [
                ("expected a valid ident after this", at.span),
            ]}]);
        }
    };
    skip_possible(source);
    let opening = try_yield!(address_span);
    match opening.token {
        Token::Equal | Token::Semicolon => {
            return err!(cont [{ UnexpectedToken, [
                ("unsure what this is meant to be", name.1),
                ("this implies a node with an address", at.span),
                ("this would be the node's address", address_span),
                (
                    "this implies an attribute, perhaps replace this with a node opening (`{`)",
                    opening.span
                ),
            ] }]);
        }
        Token::GroupOpening(GroupType::Brace) => {}
        _ => {
            let span = opening.span.clone();
            source.push(StreamResult::Ok(opening));
            return err!(cont [{ UnexpectedToken, [("expected a node opening here", span)] }]);
        }
    }
    let name_span = name.1.clone();
    let (node_name, reports) = deref_node_name((name.0, &name.1), true);
    let mut reports = reports.unwrap_or_default();
    reports.extend(errors);
    StreamResult::Ok(LexerItem {
        token: LexerToken::NodeStart {
            name: (node_name, name_span),
            unit_address: Some((address, address_span)),
            opening_delimiter: opening.span,
        },
        prevent_compilation: !reports.is_empty(),
        reports: Some(reports),
    })
}

type WarningReports = Vec<ParseErrorReport>;
type ErrorReports = Vec<ParseErrorReport>;

fn consume_attribute_value<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    eq: &Span,
) -> MultiErrorItem<(Vec<Item>, WarningReports, ErrorReports)> {
    def_try_yield!(custom_multi_err source,
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
        let token = try_yield!(eq.clone());
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
    def_try_yield!(custom_multi_err source, "this remains unclosed until eof");
    let mut res = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    loop {
        skip_possible(source);
        let token = try_yield!(lt);
        match token.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                of_type: GenericLiteralType::DecimalNumeric,
                content,
            })) => {
                res.push(Expression::NumericLiteral((
                    NumericLiteral::Decimal(content),
                    token.span,
                )));
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
                res.push(Expression::NumericLiteral((
                    NumericLiteral::Hexadecimal(content),
                    token.span,
                )));
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
    def_try_yield!(custom_multi_err source, "this remains unclosed until eof");
    let mut res = Vec::new();
    let mut errors = Vec::new();
    loop {
        skip_possible(source);
        let token = try_yield!(opening_bracket);
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
