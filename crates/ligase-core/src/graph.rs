//! The dependency graph: what feeds what, checked all at once.
//!
//! A graph is built from a step's *signature*, never by running it. The
//! Python layer reads the annotations off a decorated function and calls
//! [`Graph::add_step`]; nothing in this module can execute anything, which is
//! the point. A pipeline is type-checked before a single byte of sequence is
//! read.
//!
//! # Cycles are unrepresentable
//!
//! [`Graph::add_step`] takes [`NodeId`]s that already exist, so a node can
//! only ever depend on an earlier one. There is no cycle detection here
//! because there is nothing to detect: index order *is* dependency order, and
//! the check walks the graph in one pass with no topological sort.
//!
//! # What the graph adds to checking
//!
//! `check_step` already reports every disagreement among one step's inputs.
//! The graph adds two things that no single step can know.
//!
//! **Propagation.** A step's output carries the dimensions of its inputs,
//! re-sourced as [`Source::Derived`] — so a value renders as "from step
//! `align`" and a mistake found three steps downstream can still be traced.
//!
//! **Patience, then attribution.** `docs/dimensions.md` allows an unknown
//! dimension *unless a downstream step requires it*. A sample set that
//! nothing joins on is nobody's problem, and demanding it anyway would be a
//! false alarm. When a step does require one, the error is reported against
//! the input that has to declare it rather than the step that tripped over
//! it, once, however many steps the gap travelled through.

use std::collections::{BTreeSet, HashSet};
use std::error::Error;
use std::fmt;

use crate::check::{self, Binding, CheckError};
use crate::dim::{CoordinateSystem, Dim, Dimension, Dimensions, Source, display_path};
use crate::types::{DataKind, Dataset};

/// A handle on one node of a [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    /// A handle on the node at this position, in the order they were added.
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }

    /// This node's position, in the order nodes were added.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// One dataset argument of a step.
///
/// `declared` is the kind the step's annotation promises, which is not
/// necessarily the kind the graph will supply — catching that difference is
/// half of what checking a graph is for.
#[derive(Clone, Debug)]
pub struct Argument {
    /// The parameter it fills, as the step names it. Used in messages.
    pub param: String,
    /// The kind the step declares for that parameter.
    pub declared: DataKind,
    /// The node that supplies it.
    pub node: NodeId,
}

/// A step to add to a graph, as its signature describes it.
#[derive(Clone, Debug)]
pub struct StepSpec {
    /// The step's name, as it appears in messages.
    pub name: String,
    /// The kind of data it produces, from its return annotation.
    pub output: DataKind,
    /// The coordinate convention it writes, for the one output kind that
    /// does not imply one.
    ///
    /// Alignments and variant calls are 1-based closed by specification
    /// (decision D9), so this is ignored for them. Intervals are not: a tool
    /// that writes BED from a VCF changes the convention, and inheriting the
    /// input's would be a silent lie. Left unset, such an output reports its
    /// coordinate system as unknown.
    pub coordinates: Option<CoordinateSystem>,
    /// Its dataset arguments, in the order the signature declares them.
    ///
    /// Order matters: a dimension is inherited from the first argument that
    /// states it.
    pub arguments: Vec<Argument>,
}

/// Why a graph could not be built.
#[derive(Clone, Debug)]
pub enum GraphError {
    /// A step named a node that does not exist.
    UnknownNode {
        /// The step being added.
        step: String,
        /// The parameter whose argument was bad.
        param: String,
        /// The node it named.
        node: NodeId,
    },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode { step, param, node } => write!(
                f,
                "step `{step}` binds {param} to node {}, which is not in the graph",
                node.0
            ),
        }
    }
}

impl Error for GraphError {}

#[derive(Clone, Debug)]
enum Node {
    Input {
        label: String,
        dataset: Dataset,
    },
    Step {
        spec: StepSpec,
        /// The output dataset, propagated when the step was added.
        output: Dataset,
    },
}

/// A pipeline: loaded inputs, the steps over them, and nothing executed.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    nodes: Vec<Node>,
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Add a dataset that was loaded rather than computed.
    ///
    /// `label` names it in messages when the dataset has no path of its own.
    pub fn add_input(&mut self, label: &str, dataset: Dataset) -> NodeId {
        self.nodes.push(Node::Input {
            label: label.to_owned(),
            dataset,
        });
        NodeId(self.nodes.len() - 1)
    }

    /// Add a step over nodes that already exist, propagating its output.
    ///
    /// Nothing is executed. The output dataset is computed from the
    /// signature and the inputs' dimensions alone.
    pub fn add_step(&mut self, spec: StepSpec) -> Result<NodeId, GraphError> {
        for argument in &spec.arguments {
            if argument.node.0 >= self.nodes.len() {
                return Err(GraphError::UnknownNode {
                    step: spec.name.clone(),
                    param: argument.param.clone(),
                    node: argument.node,
                });
            }
        }

        let output = {
            let inputs: Vec<&Dataset> = spec
                .arguments
                .iter()
                .filter_map(|argument| self.dataset(argument.node))
                .collect();
            propagate(&spec, &inputs)
        };

        self.nodes.push(Node::Step { spec, output });
        Ok(NodeId(self.nodes.len() - 1))
    }

    /// The dataset a node holds or produces.
    #[must_use]
    pub fn dataset(&self, node: NodeId) -> Option<&Dataset> {
        match self.nodes.get(node.0)? {
            Node::Input { dataset, .. } => Some(dataset),
            Node::Step { output, .. } => Some(output),
        }
    }

    /// How many nodes the graph holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph holds no nodes at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The steps reachable from these outputs, in dependency order.
    #[must_use]
    pub fn step_names(&self, roots: &[NodeId]) -> Vec<&str> {
        self.reachable(roots)
            .into_iter()
            .filter_map(|index| match &self.nodes[index] {
                Node::Step { spec, .. } => Some(spec.name.as_str()),
                Node::Input { .. } => None,
            })
            .collect()
    }

    /// Check every step these outputs depend on, reporting all errors.
    ///
    /// Fixing one error per overnight run is the problem Ligase exists to
    /// remove, so nothing stops at the first.
    #[must_use]
    pub fn check(&self, roots: &[NodeId]) -> Vec<CheckError> {
        let mut errors = Vec::new();
        // One entry per (origin, dimension) already reported: a single
        // missing declaration is one thing to fix, not one error per step it
        // reaches.
        let mut demanded: HashSet<(usize, Dimension)> = HashSet::new();

        for index in self.reachable(roots) {
            let Node::Step { spec, .. } = &self.nodes[index] else {
                continue;
            };

            let mismatched = self.kind_errors(spec);
            if !mismatched.is_empty() {
                errors.extend(mismatched);
                continue;
            }

            let bindings = self.bindings(spec);
            errors.extend(check::disagreements(&spec.name, &bindings));
            self.demand(spec, &bindings, &mut demanded, &mut errors);
        }

        errors
    }

    /// Arguments whose supplied kind is not the kind the step declares.
    fn kind_errors(&self, spec: &StepSpec) -> Vec<CheckError> {
        spec.arguments
            .iter()
            .filter_map(|argument| {
                let dataset = self.dataset(argument.node)?;
                (dataset.kind() != argument.declared).then(|| {
                    check::step_type_error(
                        &spec.name,
                        &argument.param,
                        argument.declared,
                        dataset.kind(),
                        &self.describe(argument.node),
                    )
                })
            })
            .collect()
    }

    fn bindings(&self, spec: &StepSpec) -> Vec<Binding> {
        spec.arguments
            .iter()
            .filter_map(|argument| {
                Some(Binding {
                    name: argument.param.clone(),
                    dataset: self.dataset(argument.node)?.clone(),
                })
            })
            .collect()
    }

    /// Report the dimensions this step requires but does not have.
    fn demand(
        &self,
        spec: &StepSpec,
        bindings: &[Binding],
        demanded: &mut HashSet<(usize, Dimension)>,
        errors: &mut Vec<CheckError>,
    ) {
        for dimension in Dimension::ALL {
            if !check::requires(bindings, dimension) {
                continue;
            }

            for argument in &spec.arguments {
                let Some(dataset) = self.dataset(argument.node) else {
                    continue;
                };
                if !dataset.kind().applies(dimension) || dataset.dimensions().is_known(dimension) {
                    continue;
                }

                let origin = self.origin_of(argument.node, dimension);
                if !demanded.insert((origin.0, dimension)) {
                    continue;
                }

                errors.push(check::undeclared_at(
                    &spec.name,
                    dimension,
                    &self.describe(origin),
                    self.reason(origin, dimension),
                    &self.route(argument.node, origin, dimension),
                ));
            }
        }
    }

    /// Where a dimension's value — or its absence — came from.
    ///
    /// Follows the same rule propagation does: a dimension is inherited from
    /// the first argument that can speak to it. A step that *introduces* a
    /// dimension rather than inheriting it is itself the origin.
    fn origin_of(&self, node: NodeId, dimension: Dimension) -> NodeId {
        let mut node = node;
        loop {
            let Some(Node::Step { spec, .. }) = self.nodes.get(node.0) else {
                return node;
            };
            if introduces(spec, dimension) {
                return node;
            }
            match self.first_applicable(spec, dimension) {
                // Arguments always sit at lower indices, so this terminates.
                Some(next) => node = next,
                None => return node,
            }
        }
    }

    /// The steps a value passes through between its origin and `from`.
    fn route(&self, from: NodeId, origin: NodeId, dimension: Dimension) -> Vec<&str> {
        let mut steps = Vec::new();
        let mut node = from;
        while node != origin {
            let Some(Node::Step { spec, .. }) = self.nodes.get(node.0) else {
                break;
            };
            steps.push(spec.name.as_str());
            match self.first_applicable(spec, dimension) {
                Some(next) => node = next,
                None => break,
            }
        }
        steps.reverse();
        steps
    }

    /// The first argument that can speak to a dimension at all.
    fn first_applicable(&self, spec: &StepSpec, dimension: Dimension) -> Option<NodeId> {
        spec.arguments
            .iter()
            .find(|argument| {
                self.dataset(argument.node)
                    .is_some_and(|dataset| dataset.kind().applies(dimension))
            })
            .map(|argument| argument.node)
    }

    fn reason(&self, node: NodeId, dimension: Dimension) -> &'static str {
        self.dataset(node)
            .and_then(|dataset| dataset.dimensions().reason(dimension))
            .unwrap_or("not recorded")
    }

    /// How a message should name a node: the file it came from where there is
    /// one, so the user is sent to something they can edit.
    fn describe(&self, node: NodeId) -> String {
        match self.nodes.get(node.0) {
            Some(Node::Input { label, dataset }) => dataset
                .path()
                .map_or_else(|| format!("input `{label}`"), display_path),
            Some(Node::Step { spec, .. }) => format!("step `{}`", spec.name),
            None => "an unknown node".to_owned(),
        }
    }

    /// Every node these outputs depend on, in dependency order.
    fn reachable(&self, roots: &[NodeId]) -> BTreeSet<usize> {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<NodeId> = roots.to_vec();

        while let Some(node) = pending.pop() {
            if node.0 >= self.nodes.len() || !seen.insert(node.0) {
                continue;
            }
            if let Node::Step { spec, .. } = &self.nodes[node.0] {
                pending.extend(spec.arguments.iter().map(|argument| argument.node));
            }
        }

        seen
    }
}

/// Whether a step states a dimension itself rather than inheriting it.
fn introduces(spec: &StepSpec, dimension: Dimension) -> bool {
    // Coordinates are the only one. Nothing upstream of an alignment carries
    // a coordinate system — FASTQ and FASTA have none — so the step is where
    // the convention appears.
    dimension == Dimension::Coordinates && spec.output.applies(Dimension::Coordinates)
}

/// The dimensions a step's output carries, given what went into it.
///
/// Where two inputs both state a dimension, the first in signature order
/// wins. They are also checked against each other, so a disagreement is
/// already reported at this step; picking one keeps a single root cause from
/// echoing down the rest of the pipeline as a second error.
fn propagate(spec: &StepSpec, inputs: &[&Dataset]) -> Dataset {
    let step = spec.name.as_str();
    let output = spec.output;
    let mut dimensions = Dimensions::default();

    if output.applies(Dimension::Organism) {
        dimensions.organism = inherit(step, Dimension::Organism, inputs, |d| &d.organism);
    }
    if output.applies(Dimension::Reference) {
        dimensions.reference = inherit(step, Dimension::Reference, inputs, |d| &d.reference);
    }
    if output.applies(Dimension::ContigNaming) {
        dimensions.contig_naming =
            inherit(step, Dimension::ContigNaming, inputs, |d| &d.contig_naming);
    }
    if output.applies(Dimension::Coordinates) {
        dimensions.coordinates = coordinates(spec);
    }
    if output.applies(Dimension::SampleSet) {
        dimensions.sample_set = inherit(step, Dimension::SampleSet, inputs, |d| &d.sample_set);
    }
    if output.applies(Dimension::Pairing) {
        dimensions.pairing = inherit(step, Dimension::Pairing, inputs, |d| &d.pairing);
    }
    if output.applies(Dimension::QualityEncoding) {
        dimensions.quality_encoding = inherit(step, Dimension::QualityEncoding, inputs, |d| {
            &d.quality_encoding
        });
    }

    // A step's output has no path: it has not been computed, and giving it
    // one would be a claim about a file that does not exist.
    Dataset::new(output, None, dimensions)
}

/// The coordinate convention a step's output uses.
fn coordinates(spec: &StepSpec) -> Dim<CoordinateSystem> {
    let derived = Source::Derived {
        step: spec.name.clone(),
    };
    match spec.output {
        // 1-based closed by specification, in either container (D9).
        DataKind::Alignment | DataKind::Variants => {
            Dim::known(CoordinateSystem::OneBasedClosed, derived)
        }
        _ => match spec.coordinates {
            Some(system) => Dim::known(system, derived),
            None => Dim::unknown(
                "a step that writes intervals must state which coordinate \
                 system it writes; declare it on the step",
            ),
        },
    }
}

/// Take a dimension from the first input that states it.
fn inherit<T: Clone>(
    step: &str,
    dimension: Dimension,
    inputs: &[&Dataset],
    field: impl Fn(&Dimensions) -> &Dim<T>,
) -> Dim<T> {
    let applicable: Vec<&&Dataset> = inputs
        .iter()
        .filter(|dataset| dataset.kind().applies(dimension))
        .collect();

    for dataset in &applicable {
        if let Some(value) = field(dataset.dimensions()).value() {
            return Dim::known(
                value.clone(),
                Source::Derived {
                    step: step.to_owned(),
                },
            );
        }
    }

    // Nothing states it. Carry the first applicable input's gap forward
    // rather than inventing a new reason: the user needs to know where the
    // value was supposed to come from, not that this step did not have one.
    applicable.first().map_or_else(
        || Dim::unknown("no input to this step states it"),
        |dataset| field(dataset.dimensions()).clone(),
    )
}
