use crate::{
    Item, ParseErrorReport, Reference, Span, StreamResult, StreamedError,
    lexer::{LexerItem, err},
    stream_utils::StreamPrepend,
};
use indexmap::IndexMap;
use std::{collections::HashMap, rc::Rc};

#[derive(Debug)]
pub struct Node {
    pub def: (Span, Option<Span>),
    pub scope: NodeScope,
    pub omit_if_no_ref: bool,
}

#[derive(Debug, Default)]
pub struct NodeScope {
    pub properties: IndexMap<String, Property>,
    pub nodes: IndexMap<Rc<(String, Option<String>)>, Node>,
    pub delete_properties: Vec<Label>,
}

#[derive(Debug)]
pub struct Property {
    pub def: Span,
    pub value: Option<PropertyValue>,
}

type PropertyValue = Vec<Item>;

#[derive(Debug)]
pub struct RootNode {
    pub scope: NodeScope,
    pub labels: HashMap<String, LabelTarget>,
}

#[derive(Debug)]
pub struct RefNode {
    pub target_node: Reference,
    pub scope: NodeScope,
    pub this_node_labels: Vec<Label>,
    pub labels: HashMap<String, LabelTarget>,
}

type RawNodeName = String;
type RawNodeAddress = String;
type RawNodeId = (RawNodeName, Option<RawNodeAddress>);
type RawNodePath = Vec<RawNodeId>;

#[derive(Debug)]
pub enum LabelTarget {
    Node(Rc<Vec<Rc<RawNodeId>>>),
    Property(Rc<(Rc<Vec<Rc<RawNodeId>>>, String)>),
}

type SpanString = (String, Span);
type NodeName = SpanString;
type NodeAddress = SpanString;
type NodeId = (NodeName, Option<NodeAddress>);

type SpanNodeName = Span;
type SpanNodeAddress = Span;
type SpanNodeId = (SpanNodeName, Option<SpanNodeAddress>);

#[derive(Debug)]
pub enum DeleteNodeTarget {
    Direct(NodeId),
    Reference(Reference),
}

type Label = SpanString;

#[derive(Debug)]
pub enum RootItem {
    Include(SpanString),
    RootNode(NodeScope),
    RefNode(Reference),
    DeleteNode(DeleteNodeTarget),
}

type LexerOutput = StreamResult<LexerItem, StreamedError<Vec<ParseErrorReport>>>;

type Report = ParseErrorReport;
type Reports = Vec<Report>;

#[derive(Debug)]
pub enum ParsingResult<T> {
    AbortCompilation(T, Reports),
    AllowCompilation(T, Reports),
}

#[derive(Debug, Default)]
struct ParsingResultBuilder {
    reports: Reports,
    allow_compilation: bool,
}

impl ParsingResultBuilder {
    fn push_nonfatal(&mut self, report: ParseErrorReport) {
        self.reports.push(report);
    }

    fn push_fatal(&mut self, report: ParseErrorReport) {
        self.reports.push(report);
        self.allow_compilation = false;
    }

    fn extend_nonfatal(&mut self, reports: Reports) {
        self.reports.extend(reports);
    }

    fn extend_fatal(&mut self, reports: Reports) {
        if !reports.is_empty() {
            self.allow_compilation = false;
        }
        self.reports.extend(reports);
    }

    fn prevent_compilation(&mut self) {
        self.allow_compilation = false;
    }

    fn finish<T>(self, result: T) -> ParsingResult<T> {
        if self.allow_compilation {
            ParsingResult::AllowCompilation(result, self.reports)
        } else {
            ParsingResult::AbortCompilation(result, self.reports)
        }
    }
}

type OmitIfNoRef = bool;

#[derive(Debug)]
struct ScopeStackBuilderNode {
    id: Rc<(String, Option<String>)>,
    id_span: (Span, Option<Span>),
    scope: NodeScope,
    omit_if_no_ref: bool,
}

impl ScopeStackBuilderNode {
    fn new(id: NodeId, omit_if_no_ref: bool) -> Self {
        let (name, name_span) = id.0;
        let (address, address_span) = match id.1 {
            Some((address, span)) => (Some(address), Some(span)),
            None => (None, None),
        };
        Self {
            id: (name, address).into(),
            id_span: (name_span, address_span),
            omit_if_no_ref,
            scope: NodeScope::default(),
        }
    }

    fn add_property(&mut self, name: SpanString, value: Option<PropertyValue>) -> Option<Report> {
        let mut error = None;
        if !self.scope.nodes.is_empty() {
            error = Some(err!(raw [{ PropertyAfterSubnodes, [
                ("this can't appear after subnodes or node compiler directives", name.1.clone()),
            ]}]));
        }
        self.scope
            .properties
            .insert(name.0, Property { def: name.1, value });
        error
    }

    fn add_node(&mut self, id: Rc<RawNodeId>, value: Node) -> Option<Report> {
        let mut error = None;
        if let Some(prev) = self.scope.nodes.get(&id) {
            error = Some(err!(raw [{ ScopeRedefinition, [
                ("previous definition here", prev.def.0.clone()),
                ("nodes can't be re-defined in the same context", value.def.0.clone()),
            ]}]));
        }
        self.scope.nodes.insert(id, value);
        error
    }

    fn delete_property(&mut self, name: SpanString) -> Option<Report> {
        let error = if let Some(prop) = self.scope.properties.get(&name.0) {
            Some(err!(raw [{ DeleteInSameScope, [
                ("here, the property is defined in the same scope", prop.def.clone()),
                ("this is trying to delete a property in the same scope", name.1.clone()),
            ]}]))
        } else {
            None
        };
        self.scope.delete_properties.push(name);
        error
    }

    fn get_id(&self) -> Rc<(String, Option<String>)> {
        self.id.clone()
    }

    fn finish(self) -> (Rc<RawNodeId>, Node) {
        (
            self.id,
            Node {
                def: self.id_span,
                scope: self.scope,
                omit_if_no_ref: self.omit_if_no_ref,
            },
        )
    }
}

#[derive(Debug)]
struct ScopeStackBuilder {
    root: ScopeStackBuilderNode,
    stack: Vec<ScopeStackBuilderNode>,
    labels: HashMap<String, (LabelTarget, Span)>,
    path_cache: Option<Rc<Vec<Rc<RawNodeId>>>>,
}

type Labels = HashMap<String, (LabelTarget, Span)>;

impl ScopeStackBuilder {
    fn new(root_node: Span) -> Self {
        Self {
            root: ScopeStackBuilderNode::new(((String::default(), root_node), None), false),
            stack: Vec::default(),
            labels: HashMap::default(),
            path_cache: Some(Rc::new(Vec::default())),
        }
    }

    fn cache_new_path(&mut self) -> Rc<Vec<Rc<RawNodeId>>> {
        let mut res = Vec::new();
        for node in &self.stack {
            res.push(node.get_id());
        }
        let path = Rc::new(res);
        self.path_cache = Some(path.clone());
        path
    }

    fn current_path(&mut self) -> Rc<Vec<Rc<RawNodeId>>> {
        if let Some(ref cache) = self.path_cache {
            return cache.clone();
        } else {
            self.cache_new_path()
        }
    }

    fn define_label(&mut self, name: SpanString, target: LabelTarget) -> Option<Report> {
        let error = if let Some(prev) = self.labels.get(&name.0) {
            Some(err!(raw [{ ScopeRedefinition, [
                ("this was the original label definition", prev.1.clone()),
                ("this is a label re-definition", name.1.clone()),
            ]}]))
        } else {
            None
        };
        self.labels.insert(name.0, (target, name.1));
        error
    }

    fn add_node_labels(&mut self, labels: Vec<Label>) -> Reports {
        if labels.is_empty() {
            return Vec::default();
        }
        let path = self.current_path();
        let mut errors = Vec::new();
        for label in labels {
            if let Some(e) = self.define_label(label, LabelTarget::Node(path.clone())) {
                errors.push(e);
            }
        }
        errors
    }

    fn add_prop_labels(&mut self, labels: Vec<Label>, prop_name: &str) -> Reports {
        if labels.is_empty() {
            return Vec::default();
        }
        let path = self.current_path();
        let path = Rc::new((path, prop_name.to_string()));
        let mut errors = Vec::new();
        for label in labels {
            if let Some(e) = self.define_label(label, LabelTarget::Property(path.clone())) {
                errors.push(e);
            }
        }
        errors
    }

    fn invalidate_path_cache(&mut self) {
        self.path_cache = None;
    }

    fn add_node(&mut self, id: NodeId, omit_if_no_ref: bool, labels: Vec<Label>) -> Reports {
        self.invalidate_path_cache();
        self.stack
            .push(ScopeStackBuilderNode::new(id, omit_if_no_ref));
        self.add_node_labels(labels)
    }

    /// # Panics
    /// This will panic if the stack is empty. This can be checked before calling using
    /// [`Self::has_nested`].
    fn end_node(&mut self) -> Option<Report> {
        let (id, popped_node) = self.stack.pop().unwrap().finish();
        match self.stack.last_mut() {
            Some(last_node) => last_node.add_node(id, popped_node),
            None => self.root.add_node(id, popped_node),
        }
    }

    fn add_property(
        &mut self,
        name: SpanString,
        value: Option<PropertyValue>,
        labels: Vec<Label>,
    ) -> Reports {
        let mut errors = self.add_prop_labels(labels, &name.0);
        let e = match self.stack.last_mut() {
            Some(node) => node.add_property(name, value),
            None => self.root.add_property(name, value),
        };
        if let Some(e) = e {
            errors.push(e);
        }
        errors
    }

    fn has_nested(&self) -> bool {
        !self.stack.is_empty()
    }

    /// # Panics
    /// Will panic if there are nested nodes. This can be checked using [`Self::has_nested`].
    fn finish(self) -> (NodeScope, Labels) {
        if self.has_nested() {
            panic!();
        }
        (self.root.scope, self.labels)
    }
}

struct ScopeBuilder<'a, I> {
    source: &'a mut I,
}

impl<I: Iterator<Item = LexerOutput> + StreamPrepend<LexerOutput>> Iterator
    for ScopeBuilder<'_, I>
{
    type Item = StreamResult<ParsingResult<RootItem>, Reports>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut result_builder = ParsingResultBuilder::default();
        macro_rules! next {
            () => {
                match self.source.next() {
                    Some(StreamResult::Ok(v)) => Some(v),
                    Some(StreamResult::IoError(e)) => return Some(StreamResult::IoError(e)),
                    Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(e))) => {
                        return Some(StreamResult::ProcessingError(e));
                    }
                    Some(StreamResult::ProcessingError(StreamedError::CanContinue(e))) => {
                        result_builder.extend_fatal(e);
                        result_builder.prevent_compilation();
                        None
                    }
                    None => None,
                }
            };
        }
        todo!();
    }
}
