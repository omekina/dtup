use crate::{
    pointer_stream::Pos,
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        ReportInlineMessage,
    },
    result::{Errors, ParseErrorReport, StreamResult, StreamedError},
    stream_utils::StreamPrepend,
    tokenizer::{
        ArithmeticOperator, Comment, CommentType, GenericLiteralType, GroupType, LiteralToken,
        Span, SpanToken, Token, WhitespaceToken, WhitespaceTokenType,
    },
};

type StreamItem<T> = StreamResult<T, StreamedError<ParseErrorReport>>;
type TokenizerStreamItem = StreamItem<SpanToken>;

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
        expr: Expression,
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
pub enum Expression {
    ArrayEnclosed(Box<Expression>),
    CommaSeparated {
        contents: Vec<ArrayItem<Expression>>,
    },
    NodeReference {
        to: (NodeReference, Span),
    },
    String((String, Span)),
    /// Not converted to the actual values (the integer is a numeric representation of the string)
    ByteString {
        opening: Span,
        closing: Span,
        contents: Vec<ArrayItem<(u64, Span)>>,
    },
}

#[derive(Debug)]
pub enum NodeReference {
    Ident(String),
    NodePath(Vec<String>),
}

pub struct Lexer<'a, I> {
    source: &'a mut I,
}

pub struct LexerItem {
    token: LexerToken,
    reports: Option<Vec<ParseErrorReport>>,
    top_level: bool,
}

impl<I: Iterator<Item = TokenizerStreamItem>> Iterator for Lexer<'_, I> {
    type Item = Result<LexerItem, ParseErrorReport>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

fn skip_while<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
    source: &mut I,
    skipper: impl Fn(&TokenizerStreamItem) -> bool,
) {
    while let Some(v) = source.next() {
        if !skipper(&v) {
            source.push(v);
            break;
        }
    }
}

macro_rules! auto_parser {
    (skip $name: ident, $skipper: expr) => {
        fn $name<I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>>(
            source: &mut I,
        ) {
            skip_while(source, $skipper)
        }
    };

    (skip_tokens $name: ident, $skipper: expr) => {
        auto_parser!(skip $name, |v| {
            match v {
                StreamResult::Ok(SpanToken { token, .. }) => $skipper(token),
                _ => false,
            }
        });
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
                        'a'..='f' | 'A'..='F' | '0'..='9' | '_' => {}
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
                        'a'..='f' | 'A'..='F' | '0'..='9' | '_' | '+' | '-' | '.' | ',' => {}
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
                (_, Token::Hash | Token::QuestionMark) => Some(Self::Label),
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

    (end [$($t: tt)*]) => {
        StreamResult::ProcessingError(StreamedError::ShouldEnd(err!(@report $($t)*)))
    };

    (raw [$($t: tt)*]) => {
        err!(@report $($t)*)
    }
}

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
    skip_possible_inline(source);
    let next = try_yield!(start.1);
    match next.token {
        Token::At => consume_node_address(source, start, next),
        Token::Colon => todo!(),
        Token::GroupOpening(GroupType::Brace) => {
            let (name, reports) = deref_node_name((start.0, &start.1));
            StreamResult::Ok(LexerItem {
                token: LexerToken::NodeStart {
                    name: (name, start.1),
                    unit_address: None,
                    opening_delimiter: next.span,
                },
                reports,
                top_level: false,
            })
        }
        Token::Semicolon => todo!(),
        _ => todo!(),
    }
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
    def_try_yield!(source);
    skip_block_comments(source);
    let address = try_yield!(name.1);
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
            return err!(cont [{ InvalidNodeAddress, [
                ("node addresses must not contain a prefix", 2, address_span.ptr)
            ]}]);
        }
        _ => {
            return err!(cont [{ InvalidNodeAddress, [
                ("because this implies a node with an address", at.span),
                ("this must be a hexadecimal literal", address_span),
            ]}]);
        }
    }
    let address = address.content;
    skip_possible_inline(source);
    let opening = try_yield!(at.span);
    match opening.token {
        Token::Whitespace(_) | Token::Comment(_) => {
            return err!(cont [{ UnexpectedToken, [
                ("because this implies a node with address", at.span),
                ("expected a node opening after this", address_span),
            ]}]);
        }
        Token::Equal => {
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
        reports,
        top_level: false,
    })
}
