use crate::{lexer::Item, tokenizer::Span};
use std::collections::BTreeMap;

pub struct Node {
    name: SpanString,
    address: Option<SpanString>,
    properties: BTreeMap<SpanString, PropertyValue>,
    omit_if_no_ref: bool,
}

pub type SpanString = (String, Span);
pub type PropertyValue = Vec<Item>;
