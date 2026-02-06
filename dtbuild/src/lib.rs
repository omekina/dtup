use dtparse::{Item, RootScope, Span};
use indexmap::IndexMap;
use std::rc::Rc;

pub struct DeviceTree {
    root_node: Node,
}

pub struct Node {
    pub name: String,
    pub address: Option<String>,
    pub definitions: Vec<Span>,
    pub properties: IndexMap<String, Property>,
    pub nodes: IndexMap<String, Rc<Node>>,
}

pub struct Property {
    pub value: PropertyValue,
    pub definition: Span,
}

pub enum PropertyValue {
    Flag,
    Items(Rc<Vec<Item>>),
}

macro_rules! report {
    (@message_text $message: literal) => { $message.to_string() };
    (@message_text $message: expr) => { $message };

    (@message $of_type: ident ($message: expr, $span: expr)) => {
        Box::new(dtparse::report::PrimitiveReportMessage::$of_type(
            crate::report!(@message_text $message),
            $span.span,
            $span.ptr,
        )) as Box<dyn dtparse::report::ReportInlineMessage>
    };

    (@messages $of_type: ident [$($message: tt),*]) => {
        vec![$(crate::report!(@message $of_type $message)),*]
    };

    (@main_message $of_type: ident, $main_message: expr, $id: expr) => {
        dtparse::report::PrimitiveMainMessage::$of_type($main_message, $id)
    };

    (@segment ($of_type: ident, $main_message: expr, $id: expr, $messages: tt)) => {
        Box::new(dtparse::report::PrimitiveReportSegment::new(
            Some(crate::report!(@main_message $of_type, $main_message, $id)),
            crate::report!(@messages $of_type $messages),
        )) as Box<dyn dtparse::report::ReportSegment>
    };

    (@segments [$($segment: tt),*]) => {
        vec![$(crate::report!(@segment $segment)),*]
    };

    (@report $segments: tt) => {
        Box::new(dtparse::report::PrimitiveReport::new(
            crate::report!(@segments $segments)
        )) as Box<dyn dtparse::report::Report>
    };
}

use report;

macro_rules! e {
    ($id: ident $messages: tt) => {
        crate::report!(@report [(
            error,
            dtparse::errors::Errors::$id.message(),
            dtparse::errors::Errors::$id.id(),
            $messages
        )])
    };
}

pub fn build(scopes: Vec<RootScope>) -> Result<DeviceTree, dtparse::Errors> {
    todo!();
}
