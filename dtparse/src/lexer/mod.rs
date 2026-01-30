use crate::{
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        ReportInlineMessage,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError, Warnings},
    stream_utils::StreamPrepend,
    tokenizer::{
        ArithmeticOperator, Comment, CommentType, GenericLiteralType, GroupType, LiteralToken,
        Span, SpanToken, Token, WhitespaceToken, WhitespaceTokenType,
    },
};

mod expressions;

type StreamItem<T> = StreamResult<T, StreamedError<ParseErrorReport>>;
type TokenizerStreamItem = StreamItem<SpanToken>;
type MultiErrorItem<T> = StreamResult<T, StreamedError<Vec<ParseErrorReport>>>;

#[derive(Debug)]
pub enum LexerToken {
    Newline,
    CompilerDirective {
        directive: String,
        span: Span,
    },
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

#[derive(Debug)]
pub enum NumericLiteral {
    Decimal(String),
    Hexadecimal(String),
}

#[derive(Debug)]
pub enum ArithmeticOperation {
    Addition,
    Subtraction,
    Multiplication,
    Division,
    Modulo,
}

#[derive(Debug)]
pub enum RelationalOperation {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Debug)]
pub enum LogicalOperation {
    And,
    Or,
}

#[derive(Debug)]
pub enum BitwiseOperation {
    And,
    Or,
    Xor,
    LeftShift,
    RightShift,
}

#[derive(Debug)]
pub enum Expression {
    Reference((Reference, Span)),
    NumericLiteral((NumericLiteral, Span)),
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
}

#[derive(Debug)]
pub enum Item {
    Reference((Reference, Span)),
    NumericLiteral(Vec<Expression>),
    ByteString(Vec<String>),
    String((String, Span)),
}

#[derive(Debug)]
pub enum Reference {
    Label(String),
    NodePath(Vec<String>),
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
    type Item = StreamItem<LexerItem>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.source.next() {
            self.source.push(next);
            Some(consume_statement_start(self.source))
        } else {
            None
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

auto_parser!(skip_tokens skip_block_comments, |v| {
    match v {
        &Token::Comment(Comment { of_type: CommentType::Block, .. }) => true,
        _ => false,
    }
});

auto_parser!(skip_tokens skip_inline_whitespace, |v| {
    match v {
        &Token::Whitespace(
            WhitespaceToken { of_type: WhitespaceTokenType::Tab | WhitespaceTokenType::Space, .. }
        ) => true,
        _ => false,
    }
});

auto_parser!(skip_tokens skip_whitespace, |v| {
    match v {
        &Token::Whitespace(_) => true,
        _ => false,
    }
});

auto_parser!(skip_tokens skip_possible_inline, |v| {
    match v {
        &Token::Whitespace(WhitespaceToken { of_type: WhitespaceTokenType::Newline, .. }) => false,
        &Token::Comment(Comment { of_type: CommentType::Line, .. }) => false,
        &(Token::Whitespace(_) | Token::Comment(_)) => true,
        _ => false,
    }
});

auto_parser!(skip_tokens skip_possible, |v| {
    match v {
        &(Token::Whitespace(_) | Token::Comment(_)) => true,
        _ => false,
    }
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

fn consume_any_ident<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> (ExtendedIdent, Span) {
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
        if let None = ptr {
            ptr = Some(l.span.ptr);
        }
        res.push_str(&String::from(l.token));
        span += l.span.span;
        of_type = new_type;
    }

    let Some(ptr) = ptr else {
        panic!("invalid call to consume_any_ident");
    };
    let span = Span { ptr, span };
    (
        match of_type {
            ExtendedIdentType::Label => ExtendedIdent::Label(res),
            ExtendedIdentType::Node => ExtendedIdent::Node(res),
            ExtendedIdentType::Attribute => ExtendedIdent::Attribute(res),
        },
        span,
    )
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
    ($source: expr) => {
        macro_rules! try_yield {
                        ($after: expr) => {
                            match $source.next() {
                                Some(StreamResult::Ok(v)) => v,
                                Some(StreamResult::IoError(e)) => return StreamResult::IoError(e),
                                Some(StreamResult::ProcessingError(e)) => {
                                    (return StreamResult::ProcessingError(e))
                                }
                                None => {
                                    return err!(end [{ UnexpectedEof, [
                                        ("unexpected end after this", $after)
                                    ]}]);
                                }
                            }
                        };
                    }
    };
}

fn consume_statement_start<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<LexerItem> {
    def_try_yield!(source);
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
            consume_node_address(source, start, next).map(|mut item| {
                if let Some(to_push) = to_push {
                    let mut report = item.reports.unwrap_or_default();
                    report.push(to_push);
                    item.reports = Some(report);
                }
                item
            })
        }
        Token::Colon => {
            let (name, mut reports, abort) = deref_label((start.0, &start.1));
            if skipped > 0 {
                let mut reports_inner = match reports {
                    Some(v) => v,
                    None => Vec::new(),
                };
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
            let (name, reports) = deref_node_name((start.0, &start.1));
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
        Token::Equal => todo!(),
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
            err!(cont [{ UnexpectedToken, [
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
    let abort = errors.len() > 0;
    (
        label_name,
        if errors.len() > 0 { Some(errors) } else { None },
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
    match attr_name.chars().nth(0).unwrap() {
        'a'..='z' | 'A'..='Z' | '#' => {}
        _ => errors.push(PrimitiveReport::single(PrimitiveReportSegment::single(
            PrimitiveMainMessage::warning(
                Warnings::WeirdPropertyName.message(),
                Warnings::WeirdPropertyName.id(),
            ),
            PrimitiveReportMessage::warning(
                "attribute names should begin with a letter or a hashtag".to_string(),
                1,
                name.1.ptr.clone(),
            ),
        ))),
    }
    (attr_name, None, false)
}

fn deref_node_name(name: (ExtendedIdent, &Span)) -> (String, Option<Vec<ParseErrorReport>>) {
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
    match node_name.chars().nth(0).unwrap() {
        'a'..='z' | 'A'..='Z' => {}
        _ => errors.push(err!(raw [{ InvalidNodeName, [
            ("node names must start with a letter", 1, name.1.ptr.clone()),
        ]}])),
    }
    (
        node_name,
        if errors.len() > 0 { Some(errors) } else { None },
    )
}

fn consume_node_address<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    name: (ExtendedIdent, Span),
    at: SpanToken,
) -> StreamItem<LexerItem> {
    macro_rules! skip_stmt_cont {
        () => {
            auto_parser!(skip_token_inline source, |v| {
                match v {
                    &(Token::Equal | Token::GroupOpening(GroupType::Brace) | Token::Semicolon) => {
                        true
                    },
                    _ => false,
                }
            });
        };
    }
    def_try_yield!(source);
    skip_block_comments(source);
    let address = try_yield!(at.span);
    let (address, address_span) = match address.token {
        Token::Literal(LiteralToken::Ident(v)) => (v, address.span),
        Token::Whitespace(_) | Token::Comment(_) => {
            let span = address.span.clone();
            source.push(StreamResult::Ok(address));
            return err!(cont [{ UnexpectedToken, [
                ("expected a hexadecimal literal after this", span)
            ]}]);
        }
        _ => {
            let span = address.span.clone();
            source.push(StreamResult::Ok(address));
            return err!(cont [{ UnexpectedToken, [
                ("expected a hexadecimal literal here", span)
            ]}]);
        }
    };
    match address.of_type {
        GenericLiteralType::HexadecimalNumeric { prefix: false }
        | GenericLiteralType::DecimalNumeric => {}
        GenericLiteralType::HexadecimalNumeric { prefix: true } => {
            skip_stmt_cont!();
            return err!(cont [{ InvalidNodeAddress, [
                ("node addresses must not contain a prefix", 2, address_span.ptr)
            ]}]);
        }
        _ => {
            skip_stmt_cont!();
            return err!(cont [{ InvalidNodeAddress, [
                ("because this implies a node with an address", at.span),
                ("this must be a hexadecimal literal", address_span),
            ]}]);
        }
    }
    let address = address.content;
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
    let (node_name, reports) = deref_node_name((name.0, &name.1));
    StreamResult::Ok(LexerItem {
        token: LexerToken::NodeStart {
            name: (node_name, name_span),
            unit_address: Some((address, address_span)),
            opening_delimiter: opening.span,
        },
        prevent_compilation: reports.is_some(),
        reports,
    })
}
