use std::rc::Rc;

use indexmap::IndexMap;

use crate::{
    Item, ParseErrorReport, Span, StreamResult,
    lexer::{Expression, err},
    result::IoError,
    scopes::{self, NodeScope, ParsingResult, ParsingResultBuilder, Property, RootItem},
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
struct ScopeMerger {
    stack: Vec<ScopeStackItem>,
}

impl ScopeMerger {
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

impl DeviceTree {
    fn merge_scopes(
        mut first: NodeScope,
        second: NodeScope,
    ) -> WithFatalReports<NodeScope, Reports> {
        for (name, address) in second.delete_nodes {}
        todo!();
    }
}

pub fn build_tree<I: Iterator<Item = ScopeParserOutput>>(
    scope_stream: &mut I,
) -> Result<ParsingResult<DeviceTree>, IoError> {
    todo!()
}
