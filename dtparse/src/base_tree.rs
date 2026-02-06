use crate::{
    ParseErrorReport, StreamResult, StreamedError,
    lexer::{
        Item, LexerItem, LexerToken, NumericLiteral, Reference, Statement,
        compiler_directives::{CompilerDirective, NodeTarget},
        err, warning,
    },
    stream_utils::StreamPrepend,
    tokenizer::Span,
};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct RootScope {
    includes: Vec<(String, Span)>,
    nodes: Vec<Node>,
    root_delete_nodes: Vec<(NodeId, Span)>,
    root_node_properties: Vec<Property>,
    root_node_delete_properties: Vec<(String, Span)>,
    root_node_delete_nodes: Vec<(NodeId, Span)>,
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
    omit_if_no_ref: bool,
}

#[derive(Debug, Default)]
pub struct NodeScope {
    properties: Vec<Property>,
    includes: Vec<(String, Span)>,
    nodes: Vec<Node>,
    delete_properties: Vec<(String, Span)>,
    delete_nodes: Vec<(NodeId, Span)>,
}

#[derive(Debug)]
pub enum NodeId {
    Direct(DirectNodeId),
    Reference(Reference),
}

impl From<NodeTarget> for NodeId {
    fn from(value: NodeTarget) -> Self {
        match value {
            NodeTarget::Node((name, address)) => Self::Direct(DirectNodeId { name, address }),
            NodeTarget::Reference(reference) => Self::Reference(reference),
        }
    }
}

#[derive(Debug)]
pub struct DirectNodeId {
    name: SpanString,
    address: Option<SpanString>,
}

#[derive(Debug)]
pub struct Property {
    name: SpanString,
    value: Option<PropertyValue>,
}

#[derive(Debug)]
pub struct MemoryReservation {
    address: NumericLiteral,
    length: NumericLiteral,
}

pub type SpanString = (String, Span);
pub type PropertyValue = Vec<Item>;

type NodeOpening = Span;

#[derive(Clone, Debug)]
enum LabelTarget {
    Node(RawNodePath),
    Property(RawNodePath, PropertyName),
}

#[derive(Clone, Debug)]
enum PathPart {
    Node {
        name: (String, Span),
        address: Option<(String, Span)>,
    },
    Reference(Reference),
}

type RawNodeAddress = Option<String>;
type RawNodePath = Vec<PathPart>;
type PropertyName = String;

#[derive(Debug, Default)]
struct TreeStack {
    stack: Vec<(Node, NodeOpening)>,
    delete_nodes: Vec<NodeTarget>,
    labels: HashMap<String, (LabelTarget, Span)>,
}

impl TreeStack {
    fn has_nested(&self) -> bool {
        !self.stack.is_empty()
    }

    fn current_path(&self) -> RawNodePath {
        let mut res: Vec<PathPart> = Vec::new();
        for part in self.stack.iter() {
            let part = match part.0.id {
                NodeId::Direct(ref part) => PathPart::Node {
                    name: part.name.clone(),
                    address: part.address.clone(),
                },
                NodeId::Reference(ref reference) => PathPart::Reference(reference.clone()),
            };
            res.push(part);
        }
        res
    }

    fn define_label(
        &mut self,
        label: (String, Span),
        target: LabelTarget,
    ) -> Option<ParseErrorReport> {
        let (label, span) = label;
        match self.labels.get(&label) {
            Some(v) => Some(err!(raw [{ DuplicitLabel, [
                ("this is the original definition", v.1.clone()),
                ("this is a re-definition", span),
            ]}])),
            None => {
                self.labels.insert(label, (target, span));
                None
            }
        }
    }

    fn start_node(
        &mut self,
        node_id: NodeId,
        opening: NodeOpening,
        omit_if_no_ref: bool,
        labels: Vec<(String, Span)>,
    ) -> Errors {
        let error = if let NodeId::Reference(ref reference) = node_id
            && self.stack.len() != 0
        {
            Some(err!(raw [{ NestedReferenceNode, [
                ("reference nodes must only be at the top level", reference.ampersand().clone()),
            ]}]))
        } else {
            None
        };
        self.stack.push((
            Node {
                id: node_id,
                scope: NodeScope::default(),
                omit_if_no_ref,
            },
            opening,
        ));
        if labels.is_empty() {
            return error.map(|v| vec![v]).unwrap_or_default();
        }
        let path = self.current_path();
        let mut errors = Vec::new();
        if let Some(e) = error {
            errors.push(e);
        }
        for (label, span) in labels {
            if let Some(e) = self.define_label((label, span), LabelTarget::Node(path.clone())) {
                errors.push(e);
            }
        }
        errors
    }

    /// # Panics
    /// Will panic if the stack is empty. It is the caller's responsibility to check if the stack
    /// is empty. [`Self::has_nested`] can be used.
    fn end_node(&mut self) -> Option<Node> {
        let Some(popped) = self.stack.pop() else {
            panic!("end node called on an empty stack");
        };
        if let Some(parent) = self.stack.last_mut() {
            parent.0.scope.nodes.push(popped.0);
            None
        } else {
            Some(popped.0)
        }
    }

    /// # Panics
    /// Will panic if the stack is empty. It is the caller's responsibility to check if the stack
    /// is empty. [`Self::has_nested`] can be used.
    fn push_property(&mut self, attribute: Property, labels: Vec<(String, Span)>) -> Errors {
        let attr_name = if labels.is_empty() {
            None
        } else {
            Some(attribute.name.0.clone())
        };
        {
            let Some(parent) = self.stack.last_mut() else {
                panic!("push property called on an empty stack");
            };
            parent.0.scope.properties.push(attribute);
        }
        if labels.is_empty() {
            return Vec::default();
        }
        let path = self.current_path();
        let attr_name = attr_name.unwrap();
        let mut errors = Vec::new();
        for label in labels {
            if let Some(e) = self.define_label(
                label,
                LabelTarget::Property(path.clone(), attr_name.clone()),
            ) {
                errors.push(e);
            }
        }
        errors
    }

    fn delete_property(&mut self, delete_target: String, directive: Span) -> Errors {
        if let Some(last) = self.stack.last_mut() {
            last.0
                .scope
                .delete_properties
                .push((delete_target, directive));
            Vec::default()
        } else {
            vec![err!(raw [{ DeletePropertyOnRoot, [
                ("this delete property is not in a standard node", directive.clone()),
            ]}])]
        }
    }

    fn delete_node(&mut self, delete_target: NodeTarget, directive: Span) {
        if let Some(last) = self.stack.last_mut() {
            last.0
                .scope
                .delete_nodes
                .push((delete_target.into(), directive));
        } else {
            self.delete_nodes.push(delete_target);
        }
    }

    /// # Panics
    /// Will panic if the stack is empty. It is the caller's responsibility to check if the stack
    /// is empty. [`Self::has_nested`] can be used.
    fn include(&mut self, include: (String, Span)) {
        self.stack
            .last_mut()
            .expect("include call on an empty stack")
            .0
            .scope
            .includes
            .push(include);
    }

    fn finish(mut self) -> (Option<Node>, Errors) {
        let mut errors = Vec::new();
        while let Some(popped) = self.stack.pop() {
            errors.push(err!(raw [{ UnmatchedDelimiter, [
                ("this node remains unclosed", popped.1),
            ]}]));
            let Some(parent) = self.stack.last_mut() else {
                return (Some(popped.0), errors);
            };
            parent.0.scope.nodes.push(popped.0);
        }
        (None, errors)
    }
}

pub(crate) type Errors = Vec<ParseErrorReport>;
pub(crate) type PreventCompilation = bool;

#[derive(Default)]
struct ErrorTracker {
    errors: Errors,
    prevent_compilation: bool,
}

impl ErrorTracker {
    fn push_fatal(&mut self, value: ParseErrorReport) {
        self.errors.push(value);
        self.prevent_compilation = true;
    }

    fn push(&mut self, value: ParseErrorReport) {
        self.errors.push(value);
    }

    fn extend(&mut self, value: Errors) {
        self.errors.extend(value);
    }

    fn extend_fatal(&mut self, value: Errors) {
        if !value.is_empty() {
            self.prevent_compilation = true;
        }
        self.extend(value);
    }

    fn prevent_compilation(&mut self, new: bool) {
        self.prevent_compilation = self.prevent_compilation || new;
    }

    fn has_compilation_aborted(&self) -> bool {
        self.prevent_compilation
    }

    fn take(self) -> Errors {
        self.errors
    }
}

pub(crate) trait IncludeRegistry {
    fn register(&mut self, target: (String, Span));
}

pub(crate) struct IgnorantIncluder;
impl IncludeRegistry for IgnorantIncluder {
    fn register(&mut self, target: (String, Span)) {
        println!("registered include: {:?}", target.0);
    }
}

type LexerOutput = StreamResult<LexerItem, StreamedError<Vec<ParseErrorReport>>>;
pub(crate) fn preprocess_tree<I: Iterator<Item = LexerOutput> + StreamPrepend<LexerOutput>>(
    source: &mut I,
    include_file: bool,
    includer: &mut impl IncludeRegistry,
) -> StreamResult<(RootScope, PreventCompilation, Errors), Vec<ParseErrorReport>> {
    let mut scope = RootScope::default();
    let mut stack = TreeStack::default();
    let mut root_openings = Vec::new();
    let mut errors = ErrorTracker::default();
    macro_rules! try_to_item {
        ($token: expr) => {
            match $token {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(StreamedError::ShouldEnd(e)) => {
                    return StreamResult::ProcessingError(e);
                }
                StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
                    errors.extend(e);
                    continue;
                }
            }
        };
    }
    macro_rules! e {
        ($of_type: ident $e: tt) => {
            errors.push_fatal(err!(raw [{ $of_type, $e }]));
        };
    }

    // get dts header
    {
        let mut had_before = false;
        loop {
            let Some(token) = source.next() else {
                return StreamResult::IoError(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "non-dts file").into(),
                );
            };
            let LexerItem {
                token,
                reports,
                prevent_compilation,
            } = try_to_item!(token);
            errors.extend(reports.unwrap_or_default());
            errors.prevent_compilation(prevent_compilation);
            match token {
                LexerToken::CompilerDirective(CompilerDirective::DtsHeader(span)) => {
                    if had_before {
                        e!(ContentBeforeHeader [
                            ("nothing can appear before this header", span.clone()),
                        ]);
                    }
                    if include_file {
                        errors.push(warning!({ DtsHeaderInIncludeFile, [
                            ("this should not appear in a dtsi file", span),
                        ]}));
                    }
                    break;
                }
                LexerToken::Newline => {}
                _ => {
                    if include_file {
                        source.push(StreamResult::Ok(LexerItem {
                            token,
                            reports: None,
                            prevent_compilation,
                        }));
                        break;
                    }
                    had_before = true
                }
            }
        }
    }

    let mut prepend_labels: Vec<(String, Span)> = Vec::new();
    let mut omit_if_no_ref: Option<Span> = None;
    while let Some(token) = source.next() {
        let LexerItem {
            token,
            reports,
            prevent_compilation,
        } = try_to_item!(token);
        errors.extend(reports.unwrap_or_default());
        errors.prevent_compilation(prevent_compilation);
        match token {
            LexerToken::CompilerDirective(CompilerDirective::DtsHeader(span)) => {
                if include_file {
                    e!(ContentBeforeHeader [
                        ("dts headers must only appear at the beggining of include files", span),
                    ]);
                } else {
                    e!(MultipleDtsHeaders [
                        ("this is a duplicit dts header, perhaps remove it", span),
                    ]);
                }
            }

            LexerToken::CompilerDirective(CompilerDirective::Include { target, .. }) => {
                includer.register(target.clone());
                if stack.has_nested() {
                    stack.include(target);
                } else {
                    scope.includes.push(target);
                }
            }

            LexerToken::CompilerDirective(CompilerDirective::Bits { bits, .. }) => {
                e!(UnknownCompilerDirective [
                    ("this must only appear before items in property values", bits),
                ]);
            }

            LexerToken::CompilerDirective(CompilerDirective::DeleteNode {
                delete_node,
                target,
            }) => {
                if root_openings.is_empty() {
                    scope.root_delete_nodes.push((target.into(), delete_node));
                } else if stack.has_nested() {
                    stack.delete_node(target, delete_node);
                } else {
                    scope
                        .root_node_delete_nodes
                        .push((target.into(), delete_node));
                }
            }

            LexerToken::CompilerDirective(CompilerDirective::DeleteProperty {
                delete_property,
                target,
            }) => {
                if stack.has_nested() {
                    stack.delete_property(target.0, delete_property);
                } else if !root_openings.is_empty() {
                    scope.root_node_delete_properties.push(target);
                } else {
                    e!(DeletePropertyOnRoot [
                        ("this can't appear in root scope", delete_property),
                    ]);
                }
            }

            LexerToken::CompilerDirective(CompilerDirective::Memreserve {
                memreserve,
                address,
                length,
            }) => {
                if !root_openings.is_empty() {
                    e!(NestedMemreserve [
                        ("this must only appear at top level", memreserve),
                    ]);
                }
                scope
                    .memory_reservations
                    .push(MemoryReservation { address, length });
            }

            LexerToken::Invalid | LexerToken::Newline => continue,

            LexerToken::CompilerDirective(CompilerDirective::OmitIfNoRef(span)) => {
                if let Some(prev) = omit_if_no_ref.take() {
                    e!(InvalidOmitTarget [
                        ("previous one here", prev),
                        ("omit if no ref directive can't be chained", span.clone()),
                    ]);
                }
                omit_if_no_ref = Some(span);
            }

            LexerToken::RootNodeStart {
                slash,
                opening_delimiter,
            } => {
                if !prepend_labels.is_empty() {
                    let last = prepend_labels.pop().unwrap();
                    e!(InvalidLabelTarget [
                        ("labels can't refer to root nodes", last.1),
                    ]);
                    prepend_labels.clear();
                }
                if let Some(omit_if_no_ref) = omit_if_no_ref.take() {
                    e!(InvalidOmitTarget [
                        ("root nodes can't be omitted", omit_if_no_ref),
                        ("this is a root node", slash.clone()),
                    ]);
                }
                if !root_openings.is_empty() {
                    e!(NestedRootNode [
                        ("root nodes must only appear at top level", slash),
                    ]);
                }
                root_openings.push(opening_delimiter);
            }

            LexerToken::RefNodeStart {
                reference,
                opening_delimiter,
            } => {
                if !root_openings.is_empty() {
                    let last = root_openings.first().unwrap();
                    let ampersand = reference.ampersand().clone();
                    e!(InvalidNodePlacement [
                        ("the highest root node is here", last.clone()),
                        ("reference nodes must appear only at top level", ampersand),
                    ]);
                }
                let e = stack.start_node(
                    NodeId::Reference(reference),
                    opening_delimiter,
                    omit_if_no_ref.take().is_some(),
                    std::mem::take(&mut prepend_labels),
                );
                errors.extend_fatal(e);
            }

            LexerToken::NodeStart {
                name,
                unit_address,
                opening_delimiter,
            } => {
                if root_openings.is_empty() {
                    e!(InvalidNodePlacement [
                        ("standard nodes must be nested in a root node `/`", name.1.clone()),
                    ]);
                }
                let e = stack.start_node(
                    NodeId::Direct(DirectNodeId {
                        name,
                        address: unit_address,
                    }),
                    opening_delimiter,
                    omit_if_no_ref.take().is_some(),
                    std::mem::take(&mut prepend_labels),
                );
                errors.extend_fatal(e);
            }

            LexerToken::NodeEnd { closing_delimiter } => {
                if stack.has_nested() {
                    if let Some(node) = stack.end_node() {
                        scope.nodes.push(node);
                    }
                } else if !root_openings.is_empty() {
                    root_openings.pop();
                } else {
                    errors.push_fatal(err!(raw [{ UnmatchedDelimiter, [
                        ("this has no matching node opening", closing_delimiter),
                    ]}]));
                }
            }

            LexerToken::Statement(stmt) => {
                let stmt = match stmt {
                    Statement::FlagProperty { property_name } => Property {
                        name: property_name,
                        value: None,
                    },
                    Statement::PropertyAssignment {
                        property_name,
                        expr,
                        ..
                    } => Property {
                        name: property_name,
                        value: Some(expr),
                    },
                };
                if stack.has_nested() {
                    let e = stack.push_property(stmt, std::mem::take(&mut prepend_labels));
                    errors.extend_fatal(e);
                } else if !root_openings.is_empty() {
                    scope.root_node_properties.push(stmt);
                    todo!("prepend labels");
                }
            }

            LexerToken::Label { name, .. } => {
                prepend_labels.push(name);
                continue;
            }
        }

        if !prepend_labels.is_empty() {
            let last = prepend_labels.pop().unwrap();
            errors.push_fatal(err!(raw [{ InvalidLabelTarget, [
                ("a node, other label or a property must follow after this", last.1),
            ]}]));
            prepend_labels.clear();
        }
    }
    for opening in root_openings {
        errors.push_fatal(err!(raw [{ UnmatchedDelimiter, [
            ("this root node remains unclosed", opening),
        ]}]));
    }
    StreamResult::Ok((scope, errors.has_compilation_aborted(), errors.take()))
}
