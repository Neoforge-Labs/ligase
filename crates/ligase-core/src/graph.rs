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
//! **Identity under fan-out.** One written step becomes one node per sample,
//! so an error carries the node it came from, and its heading names that node
//! wherever the step's own name is ambiguous. Without it, four samples
//! failing the same way are four byte-identical messages and a count of one
//! step. With it where it is *not* needed, the specified error shape grows a
//! suffix that identifies nothing — so a name belonging to one step gets
//! none, and a name belonging to several gets one only if it tells them
//! apart.
//!
//! **Patience, then attribution.** `web/content/reference/dimensions.mdx`
//! allows an unknown dimension *unless a downstream step requires it*. A
//! sample set that nothing joins on is nobody's problem, and demanding it
//! anyway would be a false alarm. When a step does require one, the error is
//! reported against the input that has to declare it rather than the step
//! that tripped over it, once, however many steps the gap travelled through.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::check::{self, Binding, CheckError, Scope};
use crate::dim::{CoordinateSystem, Dim, Dimension, Dimensions, Source, display_path};
use crate::hivdb::frame;
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

    /// The step at this node, if it is a step rather than an input.
    ///
    /// The executor needs a step's arguments to know what to pass it; the
    /// checker reads them from inside the graph.
    #[must_use]
    pub fn step(&self, node: NodeId) -> Option<&StepSpec> {
        match self.nodes.get(node.index())? {
            Node::Step { spec, .. } => Some(spec),
            Node::Input { .. } => None,
        }
    }

    /// Every node these outputs depend on, in dependency order.
    ///
    /// Index order is dependency order -- a node can only name earlier ones
    /// -- so this needs no topological sort and a run can follow it
    /// straight through.
    #[must_use]
    pub fn reachable_nodes(&self, roots: &[NodeId]) -> Vec<NodeId> {
        self.reachable(roots)
            .into_iter()
            .map(NodeId::from_index)
            .collect()
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

        let reachable = self.reachable(roots);
        let scopes = self.scopes_within(&reachable);

        for index in reachable {
            let Node::Step { spec, .. } = &self.nodes[index] else {
                continue;
            };

            let mut found = self.kind_errors(spec);
            if found.is_empty() {
                let bindings = self.bindings(spec);
                found.extend(check::disagreements(&spec.name, &bindings));
                self.demand(spec, &bindings, &mut demanded, &mut found);
                found.extend(frame_errors(spec, &bindings));
            }

            // Every error says which node it came from, so a count of
            // failing steps counts places rather than names. The scope is
            // how a *message* says which, and only exists where the step's
            // name does not already say it.
            let scope = scopes.get(&index).cloned();
            errors.extend(
                found
                    .into_iter()
                    .map(|error| error.at_node(index, scope.clone())),
            );
        }

        errors
    }

    /// How to name each step node in a message, for the names that need it.
    ///
    /// Public because a run reports its steps the same way a check reports
    /// its errors: one written step is many nodes under fan-out, and a line
    /// that cannot say which sample it is about is a line a user cannot act
    /// on.
    ///
    /// A name belonging to one step needs nothing. The shape
    /// `web/content/project/golden-example.mdx` specifies is `in step
    /// `call`:`, and a suffix that identifies nothing a user was struggling
    /// to identify is noise on the commonest path.
    ///
    /// A name belonging to several needs something that tells *those* apart,
    /// which is the sample under per-sample fan-out. Where it is not — two
    /// nodes fed different alignments of the same sample, say — the next
    /// discriminator is tried, and if none of them separates the group then
    /// nothing is added. A suffix that repeats looks exactly like an
    /// identifier while being none, which is the bug this avoids rather than
    /// a smaller version of it.
    ///
    /// Ambiguity is judged over the nodes being checked, so a twin built for
    /// another pipeline does not start annotating this one's.
    pub fn scopes(&self, roots: &[NodeId]) -> HashMap<NodeId, Scope> {
        self.scopes_within(&self.reachable(roots))
            .into_iter()
            .map(|(index, scope)| (NodeId::from_index(index), scope))
            .collect()
    }

    fn scopes_within(&self, reachable: &BTreeSet<usize>) -> HashMap<usize, Scope> {
        let mut shared: HashMap<&str, Vec<usize>> = HashMap::new();
        for index in reachable {
            if let Some(Node::Step { spec, .. }) = self.nodes.get(*index) {
                shared.entry(spec.name.as_str()).or_default().push(*index);
            }
        }

        let mut scopes = HashMap::new();
        for nodes in shared.into_values().filter(|nodes| nodes.len() > 1) {
            if let Some(named) = self.distinguish(&nodes) {
                scopes.extend(named);
            }
        }
        scopes
    }

    /// The first discriminator that gives every one of these nodes its own
    /// distinct value.
    fn distinguish(&self, nodes: &[usize]) -> Option<Vec<(usize, Scope)>> {
        for discriminator in Discriminator::ALL {
            let Some(named) = nodes
                .iter()
                .map(|index| {
                    let value = self.discriminate(NodeId(*index), discriminator)?;
                    Some((
                        *index,
                        Scope {
                            kind: discriminator.name(),
                            value,
                        },
                    ))
                })
                .collect::<Option<Vec<(usize, Scope)>>>()
            else {
                continue;
            };

            let mut values: Vec<&str> = named.iter().map(|(_, s)| s.value.as_str()).collect();
            values.sort_unstable();
            let before = values.len();
            values.dedup();
            if values.len() == before {
                return Some(named);
            }
        }
        None
    }

    fn discriminate(&self, node: NodeId, discriminator: Discriminator) -> Option<String> {
        match discriminator {
            Discriminator::Sample => self.sample_of(node),
            Discriminator::File => self.first_file(node),
        }
    }

    /// The sample a node stands for, if it stands for exactly one.
    ///
    /// A step spanning a whole cohort stands for none: naming it after one of
    /// its samples would send a user to look at the wrong data.
    fn sample_of(&self, node: NodeId) -> Option<String> {
        let output = self.dataset(node)?;
        if output.kind().applies(Dimension::SampleSet) {
            return single_sample(output);
        }

        // The output cannot carry a sample set at all -- intervals, say -- so
        // there was nothing for it to inherit. Ask what went in, and only
        // answer if everything that can speak to it says the same sample.
        let Some(Node::Step { spec, .. }) = self.nodes.get(node.0) else {
            return None;
        };
        let mut agreed: Option<String> = None;
        for argument in &spec.arguments {
            let Some(dataset) = self.dataset(argument.node) else {
                continue;
            };
            if !dataset.kind().applies(Dimension::SampleSet) {
                continue;
            }
            match single_sample(dataset) {
                Some(sample) if agreed.as_ref().is_none_or(|seen| *seen == sample) => {
                    agreed = Some(sample);
                }
                _ => return None,
            }
        }
        agreed
    }

    /// The first file this node was built from, in signature order.
    ///
    /// Depth-first through the arguments, so a step two hops from any file
    /// still names the one its own first input came from.
    fn first_file(&self, node: NodeId) -> Option<String> {
        let mut stack = vec![node];
        let mut seen = BTreeSet::new();

        while let Some(current) = stack.pop() {
            if !seen.insert(current.0) {
                continue;
            }
            match self.nodes.get(current.0) {
                Some(Node::Input { dataset, .. }) => {
                    if let Some(path) = dataset.path() {
                        return Some(display_path(path));
                    }
                }
                // Reversed, so the first argument is the first one popped.
                Some(Node::Step { spec, .. }) => {
                    stack.extend(spec.arguments.iter().rev().map(|a| a.node));
                }
                None => {}
            }
        }
        None
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

/// What can tell two steps of the same name apart, in the order tried.
#[derive(Clone, Copy, Debug)]
enum Discriminator {
    /// The sample the step stands for. The right answer under fan-out, where
    /// one written step becomes one step per sample.
    Sample,
    /// The first file the step was built from. For same-named steps over
    /// different data from the same sample.
    File,
}

impl Discriminator {
    const ALL: [Self; 2] = [Self::Sample, Self::File];

    const fn name(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::File => "file",
        }
    }
}

/// The one sample a dataset covers, if it covers exactly one.
fn single_sample(dataset: &Dataset) -> Option<String> {
    match dataset.dimensions().sample_set.value()?.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Whether a step states a dimension itself rather than inheriting it.
/// Refuse a resistance step whose reference is not in the frame the gene
/// windows are stated in.
///
/// The one place a step's *own* coordinates are part of the engine rather
/// than of the data, so the one check that compares a reference against
/// something no input supplied. Keyed on the output kind, which is a typed
/// statement in the step's signature: a step annotated
/// `-> ResistanceReport` has said it interprets resistance, and
/// `hivdb::frame` knows what that reads in. The exception is visible here
/// for the reason `exec::BUILT_IN` is visible — an engine that knew this
/// privately would be an engine nobody could check.
fn frame_errors(spec: &StepSpec, bindings: &[Binding]) -> Vec<CheckError> {
    if spec.output != DataKind::ResistanceReport {
        return Vec::new();
    }

    bindings
        .iter()
        .filter(|binding| binding.dataset.kind() == DataKind::Reference)
        .filter_map(|binding| {
            let reference = binding.dataset.dimensions().reference.value()?;
            let mismatch = frame::mismatch(reference)?;
            Some(check::reference_frame(
                &spec.name,
                binding,
                &frame::expected(),
                mismatch.detail(),
                mismatch.conclusion(),
            ))
        })
        .collect()
}

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
