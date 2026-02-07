use crate::{
    Item, ParseErrorReport, Reference, Span, StreamResult, StreamedError,
    lexer::{
        LexerItem, LexerToken, Statement,
        compiler_directives::{CompilerDirective, NodeTarget},
        err, warning,
    },
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
    pub delete_properties: Vec<SpanString>,
    pub delete_nodes: Vec<NodeId>,
}

#[derive(Debug)]
pub struct Property {
    pub def: Span,
    pub value: Option<PropertyValue>,
}

type PropertyValue = Vec<Item>;

#[derive(Debug)]
pub struct RootNode {
    pub def: Span,
    pub scope: NodeScope,
    pub labels: LabelIndex,
}

#[derive(Debug)]
pub struct RefNode {
    pub target_node: Reference,
    pub scope: NodeScope,
    pub this_node_labels: Vec<Label>,
    pub labels: LabelIndex,
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
type LabelIndex = HashMap<String, (LabelTarget, Span)>;

#[derive(Debug)]
pub enum RootItem {
    Include(SpanString),
    RootNode(RootNode),
    RefNode(RefNode),
    DeleteNode(DeleteNodeTarget),
}

type LexerOutput = StreamResult<LexerItem, StreamedError<Vec<ParseErrorReport>>>;

type Report = ParseErrorReport;
type Reports = Vec<Report>;
type Fatal<T> = T;
type NonFatal<T> = T;

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

    fn push_opt_nonfatal(&mut self, report: Option<ParseErrorReport>) {
        if let Some(report) = report {
            self.reports.push(report);
        }
    }

    fn push_opt_fatal(&mut self, report: Option<ParseErrorReport>) {
        if let Some(report) = report {
            self.reports.push(report);
            self.allow_compilation = false;
        }
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

    fn add_property(
        &mut self,
        name: SpanString,
        value: Option<PropertyValue>,
    ) -> Option<Fatal<Report>> {
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

    fn add_node(&mut self, id: Rc<RawNodeId>, value: Node) -> Option<Fatal<Report>> {
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

    fn delete_property(&mut self, name: SpanString) -> Option<Fatal<Report>> {
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

    fn delete_node(&mut self, id: NodeId) -> Option<Fatal<Report>> {
        let (name, name_span) = id.0;
        let (address, address_span) = match id.1 {
            Some((address, span)) => (Some(address), Some(span)),
            None => (None, None),
        };
        let error = if let Some(node) = self.scope.nodes.get(&(name.clone(), address.clone())) {
            Some(err!(raw [{ DeleteInSameScope, [
                ("here, the node is defined in the same scope", node.def.0.clone()),
                ("this is trying to delete a node in the same scope", name_span.clone()),
            ]}]))
        } else {
            None
        };
        let name = (name, name_span);
        let address = match address {
            Some(v) => Some((v, address_span.unwrap())),
            None => None,
        };
        self.scope.delete_nodes.push((name, address));
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

    fn define_label(&mut self, name: SpanString, target: LabelTarget) -> Option<Fatal<Report>> {
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

    fn add_node_labels(&mut self, labels: Vec<Label>) -> Fatal<Reports> {
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

    fn add_prop_labels(&mut self, labels: Vec<Label>, prop_name: &str) -> Fatal<Reports> {
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

    fn add_node(&mut self, id: NodeId, omit_if_no_ref: bool, labels: Vec<Label>) -> Fatal<Reports> {
        self.invalidate_path_cache();
        self.stack
            .push(ScopeStackBuilderNode::new(id, omit_if_no_ref));
        self.add_node_labels(labels)
    }

    /// # Panics
    /// This will panic if the stack is empty. This can be checked before calling using
    /// [`Self::has_nested`].
    fn end_node(&mut self) -> Option<Fatal<Report>> {
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
    ) -> Fatal<Reports> {
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

    fn delete_property(&mut self, name: SpanString) -> Option<Fatal<Report>> {
        match self.stack.last_mut() {
            Some(node) => node.delete_property(name),
            None => self.root.delete_property(name),
        }
    }

    fn delete_node(&mut self, id: NodeId) -> Option<Fatal<Report>> {
        match self.stack.last_mut() {
            Some(node) => node.delete_node(id),
            None => self.root.delete_node(id),
        }
    }

    fn has_nested(&self) -> bool {
        !self.stack.is_empty()
    }

    /// # Panics
    /// Will panic if there are nested nodes. This can be checked using [`Self::has_nested`].
    fn finish(self) -> (Span, NodeScope, LabelIndex) {
        if self.has_nested() {
            panic!();
        }
        let (_, node) = self.root.finish();
        (node.def.0, node.scope, self.labels)
    }
}

#[derive(Default)]
struct PrefixBuilder {
    omit_if_no_ref: Option<Span>,
    labels: Vec<Label>,
}

impl PrefixBuilder {
    fn omit(&mut self, span: Span) -> Fatal<Reports> {
        let mut errors = Vec::new();
        if !self.labels.is_empty() {
            let last = self.labels.last().unwrap();
            errors.push(err!(raw [{ InvalidLabelTarget, [
                ("after this label, a node or a property was expected", last.1.clone()),
                ("but found an omit directive, perhaps swap them?", span.clone()),
            ]}]));
        }
        if let Some(prev) = self.omit_if_no_ref.take() {
            errors.push(err!(raw [{ ChainedOmits, [
                ("this was a previous one", prev),
                ("this is a duplicit directive", span.clone()),
            ]}]));
        }
        self.omit_if_no_ref = Some(span);
        errors
    }

    fn label(&mut self, label: SpanString) -> Option<NonFatal<Report>> {
        let warning = if let Some(last) = self.labels.last() {
            Some(warning!({ ChainedLabels, [
                ("this is a previous label", last.1.clone()),
                ("labels shouldn't be chained", label.1.clone()),
            ]}))
        } else {
            None
        };
        self.labels.push(label);
        warning
    }

    fn node(self) -> (Vec<Label>, OmitIfNoRef) {
        (self.labels, self.omit_if_no_ref.is_some())
    }

    fn attribute(self, name: &Span) -> (Vec<Label>, Option<Fatal<Report>>) {
        let error = if let Some(omit) = self.omit_if_no_ref {
            Some(err!(raw [{ InvalidOmitTarget, [
                ("this can't target a property", omit),
                ("this, however, is a property", name.clone()),
            ]}]))
        } else {
            None
        };
        (self.labels, error)
    }

    fn other(mut self) -> Vec<Fatal<Report>> {
        let mut errors = Vec::new();
        if let Some(omit) = self.omit_if_no_ref {
            errors.push(err!(raw [{ InvalidOmitTarget, [
                ("expected a node after this (or labels and then a node)", omit),
            ]}]));
        }
        if let Some(label) = self.labels.pop() {
            errors.push(err!(raw [{ InvalidLabelTarget, [
                ("expected a node or a property after this label", label.1),
            ]}]));
        }
        errors
    }
}

pub struct ScopeBuilder<'a, I> {
    source: &'a mut I,
    dts_header: Option<Span>,
    has_content: bool,
    is_include_file: bool,
}

impl<'a, I> ScopeBuilder<'a, I> {
    pub fn new(source: &'a mut I, is_include_file: bool) -> Self {
        Self {
            source,
            dts_header: None,
            has_content: false,
            is_include_file,
        }
    }
}

macro_rules! def_next {
    (@def $source: expr, $result_builder: expr, $mode: ident) => {
        macro_rules! next {
            () => {
                def_next!(@inner_match $source, $result_builder, $mode)
            };
        }
    };

    (@err raw $e: expr) => { $e };
    (@err option $e: expr) => { Some($e) };

    (@inner_match $source: expr, $result_builder: expr, $mode: ident) => {
        match $source.next() {
            Some(StreamResult::Ok(v)) => Some(v),
            Some(StreamResult::IoError(e)) => return def_next!(@err $mode StreamResult::IoError(e)),
            Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(e))) => {
                return def_next!(@err $mode StreamResult::ProcessingError(e));
            }
            Some(StreamResult::ProcessingError(StreamedError::CanContinue(e))) => {
                $result_builder.extend_fatal(e);
                $result_builder.prevent_compilation();
                None
            }
            None => None,
        }
    };

    (raw $source: expr, $result_builder: expr) => {
        def_next!(@def $source, $result_builder, raw)
    };

    (option $source: expr, $result_builder: expr) => {
        def_next!(@def $source, $result_builder, option)
    };
}

macro_rules! yeet_value {
    ($v: expr) => {
        match $v {
            StreamResult::Ok(v) => v,
            StreamResult::IoError(e) => return StreamResult::IoError(e),
            StreamResult::ProcessingError(e) => return StreamResult::ProcessingError(e),
        }
    };
}

macro_rules! def_err {
    ($result_builder: expr) => {
        macro_rules! e {
                            ($message: ident $messages: tt) => {
                                $result_builder.push_fatal(err!(raw [{ $message, $messages }]));
                            };
                        }
    };
}

impl<I: Iterator<Item = LexerOutput> + StreamPrepend<LexerOutput>> ScopeBuilder<'_, I> {
    fn dts_header(&mut self, span: Span) -> Option<Report> {
        if let Some(ref first) = self.dts_header {
            return Some(err!(raw [{ MultipleDtsHeaders, [
                ("this was the first dts header", first.clone()),
                ("a file can't have multiple dts headers", span),
            ]}]));
        }
        if self.has_content {
            return Some(err!(raw [{ ContentBeforeHeader, [
                ("this header must appear at the start of the file", span),
            ]}]));
        }
        if self.is_include_file {
            return Some(warning!({ DtsHeaderInIncludeFile, [
                ("includes files should not have dts headers", span),
            ]}));
        }
        None
    }

    /// # Returns
    /// Whether the outer loop should continue (`true`), or the main node was ended and the outer
    /// loop should quit (`false`).
    fn node_token(
        &mut self,
        token: LexerToken,
        result_builder: &mut ParsingResultBuilder,
        prefixer: &mut PrefixBuilder,
        stack: &mut ScopeStackBuilder,
    ) -> bool {
        def_err!(result_builder);
        match token {
            LexerToken::Invalid | LexerToken::Newline => true,

            LexerToken::CompilerDirective(CompilerDirective::DeleteProperty {
                target,
                ..
            }) => {
                result_builder.extend_fatal(std::mem::take(prefixer).other());
                result_builder.push_opt_fatal(stack.delete_property(target));
                true
            }
            LexerToken::CompilerDirective(CompilerDirective::DeleteNode {
                delete_node,
                target,
            }) => {
                result_builder.extend_fatal(std::mem::take(prefixer).other());
                let target = match target {
                    NodeTarget::Node(direct) => direct,
                    NodeTarget::Reference(reference) => {
                        let ampersand = reference.ampersand().clone();
                        e!(NestedReferencedNodeDelete [
                            ("this can't be nested", delete_node),
                            ("because it points to a node through a reference", ampersand),
                        ]);
                        return true;
                    }
                };
                result_builder.push_opt_fatal(stack.delete_node(target));
                true
            }

            LexerToken::CompilerDirective(CompilerDirective::OmitIfNoRef(omit)) => {
                result_builder.extend_fatal(prefixer.omit(omit));
                true
            }
            LexerToken::Label { name, .. } => {
                result_builder.push_opt_nonfatal(prefixer.label(name));
                true
            }

            LexerToken::Statement(Statement::FlagProperty { property_name }) => {
                let (labels, error) = std::mem::take(prefixer).attribute(&property_name.1);
                result_builder.push_opt_fatal(error);
                stack.add_property(property_name, None, labels);
                true
            }
            LexerToken::Statement(Statement::PropertyAssignment {
                property_name,
                expr,
                ..
            }) => {
                let (labels, error) = std::mem::take(prefixer).attribute(&property_name.1);
                result_builder.push_opt_fatal(error);
                stack.add_property(property_name, Some(expr), labels);
                true
            }

            LexerToken::RefNodeStart { reference, .. } => {
                e!(NestedReferenceNode [
                    ("reference nodes can't be nested", reference.ampersand().clone()),
                ]);
                true
            }
            LexerToken::RootNodeStart { slash, .. } => {
                e!(NestedReferenceNode [
                    ("root nodes can't be nested", slash),
                ]);
                true
            }

            LexerToken::NodeStart {
                name,
                unit_address,
                ..
            } => {
                let (labels, omit) = std::mem::take(prefixer).node();
                result_builder.extend_fatal(stack.add_node((name, unit_address), omit, labels));
                true
            }
            LexerToken::NodeEnd { .. } => {
                result_builder.extend_fatal(std::mem::take(prefixer).other());
                if stack.has_nested() {
                    result_builder.push_opt_fatal(stack.end_node());
                    true
                } else {
                    false
                }
            }

            LexerToken::CompilerDirective(directive) => {
                e!(UnknownCompilerDirective [
                    (
                        "even though this is a valid directive, it can't be used in node scopes",
                        directive.ident_span().clone()
                    ),
                ]);
                true
            }
        }
    }

    fn node(
        &mut self,
        name: Span,
        opening: Span,
        result_builder: &mut ParsingResultBuilder,
    ) -> StreamResult<(Span, NodeScope, LabelIndex), Reports> {
        def_next!(raw self.source, result_builder);
        def_err!(result_builder);
        let mut stack = ScopeStackBuilder::new(name);
        let mut prefixer = PrefixBuilder::default();
        let mut closed = false;
        while let Some(token) = next!() {
            result_builder.extend_nonfatal(token.reports.unwrap_or_default());
            if token.prevent_compilation {
                result_builder.prevent_compilation();
            }
            if !self.node_token(token.token, result_builder, &mut prefixer, &mut stack) {
                closed = true;
                break;
            }
        }
        if !closed {
            e!(UnmatchedDelimiter [
                ("this is unclosed until eof", opening),
            ]);
        }
        StreamResult::Ok(stack.finish())
    }

    fn ref_node(
        &mut self,
        target: Reference,
        opening: Span,
        this_node_labels: Vec<Label>,
    ) -> StreamResult<ParsingResult<RootItem>, Reports> {
        let mut result_builder = ParsingResultBuilder::default();
        let (_, scope, labels) =
            yeet_value!(self.node(target.ampersand().clone(), opening, &mut result_builder));
        StreamResult::Ok(result_builder.finish(RootItem::RefNode(RefNode {
            target_node: target,
            scope,
            this_node_labels,
            labels,
        })))
    }

    fn root_node(
        &mut self,
        name: Span,
        opening: Span,
    ) -> StreamResult<ParsingResult<RootItem>, Reports> {
        let mut result_builder = ParsingResultBuilder::default();
        let (def, scope, labels) = yeet_value!(self.node(name, opening, &mut result_builder));
        StreamResult::Ok(result_builder.finish(RootItem::RootNode(RootNode { def, scope, labels })))
    }
}

impl<I: Iterator<Item = LexerOutput> + StreamPrepend<LexerOutput>> Iterator
    for ScopeBuilder<'_, I>
{
    type Item = StreamResult<ParsingResult<RootItem>, Reports>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!();
    }
}
