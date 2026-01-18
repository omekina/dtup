use crate::{
    result::{ParseErrorReport, StreamResult, StreamedError},
    tokenizer::{Span, SpanToken},
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
        unit_address: (String, Span),
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
