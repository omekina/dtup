use std::{collections::HashMap, rc::Rc};

use indexmap::IndexMap;

use crate::{
    ParseErrorReport, Reference, Span, StreamResult,
    lexer::err,
    result::IoError,
    scopes::{self, LabelTarget, ParsingResult, ParsingResultBuilder, Property, RootItem},
};

pub trait Includer {
    fn hint_include(&mut self, target: &str) -> Result<(), String>;
}

type SpanString = (String, Span);
type NodeName = SpanString;
type NodeAddress = SpanString;
type NodeId = (NodeName, Option<NodeAddress>);
type RawNodeName = String;
type RawNodeAddress = String;
type RawNodeId = (RawNodeName, Option<RawNodeAddress>);

type Address = SpanString;
type Length = SpanString;
type MemoryReservation = (Address, Length);

#[derive(Default)]
pub struct DeviceTree {
    memory_reservations: Vec<MemoryReservation>,
    root: Node,
}

#[derive(Default)]
pub struct Node {
    pub defs: Vec<Span>,
    pub nodes: IndexMap<RawNodeId, Node>,
    pub properties: IndexMap<String, Property>,
}

type Report = ParseErrorReport;
type Reports = Vec<Report>;

type Fatal<T> = T;
type NonFatal<T> = T;

type ScopeParserOutput = StreamResult<ParsingResult<RootItem>, Reports>;

type WithFatalReports<T, R> = (T, Fatal<R>);
type WithNonFatalReports<T, R> = (T, NonFatal<R>);

type IndexMapNodeIterator = indexmap::map::IntoIter<Rc<RawNodeId>, scopes::Node>;
type ScopeStackItem = (RawNodeId, Node, IndexMapNodeIterator);

#[derive(Default)]
struct LabelTracker {
    labels: HashMap<String, (scopes::LabelTarget, Span)>,
}

type SharedNodePath = Rc<Vec<Rc<RawNodeId>>>;
type NodePath = Vec<NodeId>;

impl LabelTracker {
    fn extend(
        &mut self,
        next: HashMap<String, (scopes::LabelTarget, Span)>,
        result: &mut ParsingResultBuilder,
    ) {
        for (name, target) in next {
            self.push(name, target, result);
        }
    }

    fn push(
        &mut self,
        name: String,
        target: (scopes::LabelTarget, Span),
        result: &mut ParsingResultBuilder,
    ) {
        if let Some(prev) = self.labels.get(&name) {
            result.push_fatal(err!(raw [{ DuplicitLabel, [
                ("previous definition is here", prev.1.clone()),
                ("this is a label re-definition", target.1.clone()),
            ]}]))
        } else {
            self.labels.insert(name, target);
        }
    }

    fn req_node_target(
        &self,
        name: &str,
        referrer: &Span,
    ) -> Result<SharedNodePath, Fatal<Report>> {
        match self.labels.get(name) {
            Some((scopes::LabelTarget::Node(v), _)) => Ok(v.clone()),
            Some((scopes::LabelTarget::Property(_), span)) => Err(err!(raw [{ NotANodeLabel, [
                ("this label does not point to a node, but a property", span.clone()),
                ("this needs the label to point to a node", referrer.clone()),
            ]}])),
            None => Err(err!(raw [{ UnknownLabel, [
                ("this label wasn't found in this file", referrer.clone()),
            ]}])),
        }
    }
}

#[derive(Default)]
struct ScopeMerger {
    stack: Vec<ScopeStackItem>,
}

type ParentNode<'a> = &'a mut Node;

impl ScopeMerger {
    fn merge(
        id: RawNodeId,
        prev: Node,
        next: scopes::Node,
        result: &mut ParsingResultBuilder,
    ) -> (RawNodeId, Node) {
        let mut merger = Self::new(id, prev, next, result);
        loop {
            match merger.step(result) {
                Some(v) => break v,
                None => {}
            }
        }
    }

    fn merge_root_nodes(
        prev_root: Node,
        mut next: scopes::RootNode,
        labels: &mut LabelTracker,
        result: &mut ParsingResultBuilder,
    ) -> Node {
        labels.extend(std::mem::take(&mut next.labels), result);
        Self::merge(("".to_string(), None), prev_root, next.into(), result).1
    }

    fn push_ref_node_labels(
        this_labels: Vec<(String, Span)>,
        labels: &mut LabelTracker,
        path: SharedNodePath,
        result: &mut ParsingResultBuilder,
    ) {
        for label in this_labels {
            labels.push(label.0, (LabelTarget::Node(path.clone()), label.1), result);
        }
    }

    fn merge_ref_node(
        mut prev_root: Node,
        labels: &mut LabelTracker,
        next: scopes::RefNode,
        result: &mut ParsingResultBuilder,
    ) -> Result<(), Fatal<Reports>> {
        let ampersand = next.target_node.ampersand().clone();
        let (root, nested_id, target) = match next.target_node {
            Reference::Label(name, span, _) => {
                let path = match labels.req_node_target(&name, &span) {
                    Ok(v) => v,
                    Err(e) => return Err(vec![e]),
                };
                Self::push_ref_node_labels(next.this_node_labels, labels, path.clone(), result);
                Self::req_at_path(&mut prev_root, path)
            }
            Reference::NodePath(path, _) => {
                let path_shared: Option<SharedNodePath> = if next.this_node_labels.is_empty() {
                    None
                } else {
                    Some(Rc::new(
                        path.iter()
                            .map(|v| Rc::new((v.0.0.clone(), v.1.clone().map(|v| v.0))))
                            .collect::<Vec<_>>(),
                    ))
                };
                match Self::at_path(&mut prev_root, path) {
                    Ok(v) => {
                        let this_labels = next.this_node_labels;
                        if !this_labels.is_empty() {
                            let path = path_shared.unwrap();
                            Self::push_ref_node_labels(this_labels, labels, path, result);
                        }
                        v
                    }
                    Err(e) => return Err(vec![e]),
                }
            }
        };
        let next = scopes::Node {
            def: (ampersand, None),
            scope: next.scope,
            omit_if_no_ref: false,
        };
        let (id, merged) = Self::merge(nested_id, target, next, result);
        root.nodes.insert(id, merged);
        Ok(())
    }

    /// # Panics
    /// Will panic if the path doesn't exist
    fn req_at_path(root: &mut Node, path: SharedNodePath) -> (ParentNode<'_>, RawNodeId, Node) {
        let mut parent = root;
        let last = path.last().unwrap();
        for part in &path[..path.len() - 1] {
            parent = parent.nodes.get_mut(&**part).unwrap();
        }
        let node = parent.nodes.shift_remove(&**last).unwrap();
        (parent, (**last).clone(), node)
    }

    fn get_nested(parent: &mut Node, id: NodeId) -> Result<&mut Node, Fatal<Report>> {
        let (address, _) = match id.1 {
            Some((a, s)) => (Some(a), Some(s)),
            None => (None, None),
        };
        let raw_id = (id.0.0, address);
        match parent.nodes.get_mut(&raw_id) {
            Some(v) => Ok(v),
            None => Err(err!(raw [{ NodeNotFound, [
                ("a matching node was not found in this scope", id.0.1),
            ]}])),
        }
    }

    /// # Panics
    /// If the path is empty, this will panic. This should be checked by the caller. As root node
    /// here can't be moved out.
    fn at_path(
        root: &mut Node,
        mut path: NodePath,
    ) -> Result<(ParentNode<'_>, RawNodeId, Node), Fatal<Report>> {
        let mut parent = root;
        let last = path.pop().unwrap();
        for part in path {
            parent = match Self::get_nested(parent, part) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
        }
        let (last_address, _) = match last.1 {
            Some((a, s)) => (Some(a), Some(s)),
            None => (None, None),
        };
        let last_raw_id = (last.0.0, last_address);
        match parent.nodes.shift_remove(&last_raw_id) {
            Some(v) => Ok((parent, last_raw_id, v)),
            None => Err(err!(raw [{ NodeNotFound, [
                ("a matching node was not found in this scope", last.0.1),
            ]}])),
        }
    }

    fn new(
        id: RawNodeId,
        prev: Node,
        next: scopes::Node,
        result: &mut ParsingResultBuilder,
    ) -> Self {
        let (parent, nested) = Self::merge_node(Some(prev), next, result);
        Self {
            stack: vec![(id, parent, nested.into_iter())],
        }
    }

    /// # Returns
    /// The root node, when ended.
    ///
    /// # Panics
    /// If the stack is empty---at least the root node should be present.
    fn step(&mut self, result: &mut ParsingResultBuilder) -> Option<(RawNodeId, Node)> {
        let (_, parent, iterator) = self.stack.last_mut().unwrap();
        if let Some((id, node)) = iterator.next() {
            let to_push = Self::merge_and_nest(parent, (*id).clone(), node, result);
            self.stack.push(to_push);
            None
        } else {
            self.merge_up()
        }
    }

    fn convert_node(
        node: scopes::Node,
        result: &mut ParsingResultBuilder,
    ) -> (Node, IndexMap<Rc<RawNodeId>, scopes::Node>) {
        const NO_PREV_DELETE: &str = "this delete is inside a scope that was never defined before, hence it has nothing to delete";
        for delete in node.scope.delete_nodes.into_iter() {
            result.push_fatal(err!(raw [{ DeleteTargetNotFound, [
                (NO_PREV_DELETE.to_string(), delete.0.1),
            ]}]));
        }
        for delete in node.scope.delete_properties.into_iter() {
            result.push_fatal(err!(raw [{ DeleteTargetNotFound, [
                (NO_PREV_DELETE.to_string(), delete.1),
            ]}]));
        }
        (
            Node {
                defs: vec![node.def.0],
                nodes: IndexMap::default(),
                properties: node.scope.properties,
            },
            node.scope.nodes,
        )
    }

    fn merge_node(
        prev: Option<Node>,
        node: scopes::Node,
        result: &mut ParsingResultBuilder,
    ) -> (Node, IndexMap<Rc<RawNodeId>, scopes::Node>) {
        if let Some(mut prev) = prev {
            for delete in node.scope.delete_nodes {
                if let None = prev
                    .nodes
                    .shift_remove(&(delete.0.0, delete.1.map(|v| v.0)))
                {
                    result.push_fatal(err!(raw [{ DeleteTargetNotFound, [
                        ("this target node wasn't defined in this scope before", delete.0.1),
                    ]}]))
                }
            }
            for delete in node.scope.delete_properties {
                if let None = prev.properties.shift_remove(&delete.0) {
                    result.push_fatal(err!(raw [{ DeleteTargetNotFound, [
                        ("this target property wasn't defined in this scope before", delete.1),
                    ]}]));
                }
            }
            prev.properties.extend(node.scope.properties);
            prev.defs.push(node.def.0);
            (prev, node.scope.nodes)
        } else {
            Self::convert_node(node, result)
        }
    }

    /// Creates a nested entry on the stack or starts extending a present one
    ///
    /// # Panics
    /// Will panic if the stack is empty
    fn merge_and_nest(
        parent: &mut Node,
        id: RawNodeId,
        node: scopes::Node,
        result: &mut ParsingResultBuilder,
    ) -> ScopeStackItem {
        let prev = parent.nodes.shift_remove(&id);
        let (merged, nested) = Self::merge_node(prev, node, result);
        (id, merged, nested.into_iter())
    }

    /// # Returns
    /// The parent node and it's id (if there are no more parent nodes in the stack).
    ///
    /// # Caution
    /// The last iterator in the stack should, ideally, be empty.
    ///
    /// # Panics
    /// Will panic if the stack if empty
    fn merge_up(&mut self) -> Option<(RawNodeId, Node)> {
        let (id, leaf, _) = self.stack.pop().unwrap();
        if let Some(parent) = self.stack.last_mut() {
            parent.1.nodes.insert(id, leaf);
            None
        } else {
            Some((id, leaf))
        }
    }
}

impl DeviceTree {}

pub fn build_tree<I: Iterator<Item = ScopeParserOutput>>(
    scope_stream: &mut I,
) -> Result<ParsingResult<DeviceTree>, IoError> {
    todo!()
}
