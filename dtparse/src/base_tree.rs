use crate::{
    ParseErrorReport, StreamResult, StreamedError,
    lexer::{Item, LexerItem, NumericLiteral, Reference, compiler_directives::NodeTarget},
    tokenizer::Span,
};

#[derive(Debug)]
pub struct RootScope {
    nodes: Vec<Node>,
    delete_nodes: Vec<NodeTarget>,
    memory_reservations: Vec<MemoryReservation>,
}

#[derive(Debug)]
pub enum RootScopeNodeId {
    Slash,
    Reference(Reference),
}

#[derive(Debug)]
pub struct Node {
    id: NodeId,
    scope: NodeScope,
}

#[derive(Debug)]
pub struct NodeScope {
    properties: Vec<Property>,
    nodes: Vec<Node>,
    delete_properties: Vec<SpanString>,
    delete_nodes: Vec<NodeTarget>,
    omit_if_no_ref: bool,
}

#[derive(Debug)]
pub enum NodeId {
    Direct(DirectNodeId),
    Reference(Reference),
}

#[derive(Debug)]
pub struct DirectNodeId {
    name: SpanString,
    address: SpanString,
}

#[derive(Debug)]
pub struct Property {
    name: SpanString,
    value: PropertyValue,
}

#[derive(Debug)]
pub struct MemoryReservation {
    address: NumericLiteral,
    length: NumericLiteral,
}

pub type SpanString = (String, Span);
pub type PropertyValue = Vec<Item>;

struct TreeStack {
    stack: Vec<Node>,
}

pub fn preprocess_tree<
    I: Iterator<Item = StreamResult<LexerItem, StreamedError<Vec<ParseErrorReport>>>>,
>(
    source: &mut I,
) -> StreamResult<RootScope, StreamedError<Vec<ParseErrorReport>>> {
    todo!();
}
