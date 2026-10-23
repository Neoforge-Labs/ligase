//! Dimension checking: refusing to connect steps whose dimensions disagree.
//!
//! Two notions are kept apart, and the distinction is the whole design.
//!
//! - **Checked**: whenever two inputs both state a value for a dimension,
//!   those values must agree. Always, for every dimension. This costs
//!   nothing and demands nothing.
//! - **Required**: additionally, the value must not be unknown. Only for the
//!   pairs in [`required_pair`], where a disagreement would silently corrupt
//!   results.
//!
//! `web/content/reference/dimensions.mdx` says unknown dimensions are allowed
//! only if no downstream step requires them. Checking one step in isolation
//! has no downstream, so [`check_step`] takes "required" from the kinds being
//! joined. A whole pipeline does have one: [`crate::graph`] uses
//! [`disagreements`] and [`requires`] to decide for itself, and attributes a
//! gap to the input that has to declare it rather than to the step that
//! tripped over it. The split is what lets the golden example work:
//! `lg.samples()` takes no organism, and `align` is the step that
//! *introduces* the reference, so demanding a declared organism on the FASTQs
//! would break a documented API for no safety gain.
//!
//! Erring toward demands would be the worse failure. A checker that cries
//! wolf gets bypassed, and a bypassed checker catches nothing.

use std::error::Error;
use std::fmt;

use crate::dim::{
    ContigNaming, Dimension, Dimensions, NameSet, ReferenceId, Relation, Verdict, english_list,
    relate,
};
use crate::types::{DataKind, Dataset};

/// One named input to a step.
#[derive(Clone, Debug)]
pub struct Binding {
    /// The name the step knows it by, used in error messages.
    pub name: String,
    /// The data itself.
    pub dataset: Dataset,
}

/// What kind of disagreement an error reports.
///
/// These map onto the Python exception classes, so a caller can catch a
/// contig-naming problem without catching every dimension problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Two values for the same dimension disagree.
    Dimension,
    /// The same contigs under different names.
    ContigNaming,
    /// The same contigs in a different order.
    ContigOrder,
    /// A reference is not numbered along the frame a step's own coordinates
    /// are stated in.
    ReferenceFrame,
    /// A dimension the step requires is unknown.
    Undeclared,
    /// A step was handed a kind of data it does not declare.
    StepType,
}

impl ErrorKind {
    /// The heading this kind of error leads with.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Dimension => "DimensionError",
            Self::ContigNaming => "ContigNamingError",
            Self::ContigOrder => "ContigOrderError",
            Self::ReferenceFrame => "ReferenceFrameError",
            Self::Undeclared => "UndeclaredDimensionError",
            Self::StepType => "StepTypeError",
        }
    }

    /// The name used in the Python API to pick an exception class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dimension => "dimension",
            Self::ContigNaming => "contig_naming",
            Self::ContigOrder => "contig_order",
            Self::ReferenceFrame => "reference_frame",
            Self::Undeclared => "undeclared",
            Self::StepType => "step_type",
        }
    }
}

/// Which instance of a step an error is about, when one step runs many times.
///
/// Under per-sample fan-out a single written step becomes one node per
/// sample, and the node *is* the sample. An error that does not say which one
/// leaves a user unable to tell one problem from four, or to know which
/// samples were hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// What kind of thing distinguishes it, e.g. `sample`.
    pub kind: &'static str,
    /// Which one, e.g. `SRR32217714`.
    pub value: String,
}

impl Scope {
    /// How this reads in a message: `sample SRR32217714`.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} {}", self.kind, self.value)
    }

    /// The plural of [`Self::kind`], for a line about several of them.
    #[must_use]
    pub fn kinds(&self) -> String {
        format!("{}s", self.kind)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

/// One disagreement found by [`check_step`].
///
/// The rendered message names the step, both conflicting values, where each
/// value came from, and asks a leading question — the shape specified at the
/// end of `web/content/project/golden-example.mdx`.
#[derive(Clone, Debug)]
pub struct CheckError {
    kind: ErrorKind,
    step: String,
    /// Which node of a graph it came from, when a graph found it.
    ///
    /// This is what makes a count of failing *steps* honest: one step name
    /// can be many steps.
    node: Option<usize>,
    /// How to name that node, when it can be named.
    scope: Option<Scope>,
    dimension: Option<Dimension>,
    lines: Vec<String>,
    detail: Option<String>,
    conclusion: String,
}

impl CheckError {
    /// What kind of disagreement this is.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The step it was found in.
    #[must_use]
    pub fn step(&self) -> &str {
        &self.step
    }

    /// Which node of a graph this came from, when a graph found it.
    #[must_use]
    pub const fn node(&self) -> Option<usize> {
        self.node
    }

    /// How that node is named, when it can be named.
    #[must_use]
    pub const fn scope(&self) -> Option<&Scope> {
        self.scope.as_ref()
    }

    /// Record which node of a graph this came from, and how to name it.
    ///
    /// Only a graph can know either. A single step checked on its own has no
    /// node and nothing to distinguish it from itself.
    #[must_use]
    pub fn at_node(mut self, node: usize, scope: Option<Scope>) -> Self {
        self.node = Some(node);
        self.scope = scope;
        self
    }

    /// The dimension that disagrees, for the errors that are about one.
    ///
    /// `None` for a [`ErrorKind::StepType`] error: a step handed a VCF where
    /// it declared a FASTA has no dimension to blame.
    #[must_use]
    pub const fn dimension(&self) -> Option<Dimension> {
        self.dimension
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&write_error(self, true))
    }
}

/// The error as text, with or without the scope in its heading.
///
/// Without it, the text is the *problem* rather than the problem at one
/// sample, which is how [`render`] tells one problem affecting four samples
/// from four different problems.
fn write_error(error: &CheckError, scoped: bool) -> String {
    let mut out = format!("{} in step `{}`", error.kind.heading(), error.step);
    if scoped && let Some(scope) = &error.scope {
        out.push_str(&format!(" ({scope})"));
    }
    out.push(':');
    for line in &error.lines {
        out.push_str("\n  ");
        out.push_str(line);
    }
    if let Some(detail) = &error.detail {
        out.push_str("\n  ");
        out.push_str(detail);
    }
    out.push_str("\n  ");
    out.push_str(&error.conclusion);
    out
}

impl Error for CheckError {}

/// Check every dimension of every pair of inputs to a step.
///
/// Returns *all* disagreements rather than stopping at the first: M2 requires
/// `ligase check` to report every type error in one pass, and a user fixing
/// one error at a time through an overnight run is the problem Ligase exists
/// to remove.
#[must_use]
pub fn check_step(step: &str, bindings: &[Binding]) -> Vec<CheckError> {
    let mut errors = Vec::new();
    for dimension in Dimension::ALL {
        check_dimension(step, bindings, dimension, &mut errors);
    }
    errors
}

/// Only the disagreements: values that are both stated and do not agree.
///
/// This is the half of [`check_step`] that needs no notion of what is
/// required. [`crate::graph`] uses it because it decides requirements from
/// the pipeline rather than from the pair of kinds in front of it.
#[must_use]
pub fn disagreements(step: &str, bindings: &[Binding]) -> Vec<CheckError> {
    let mut errors = Vec::new();
    for dimension in Dimension::ALL {
        let Some(applicable) = joined_on(bindings, dimension) else {
            continue;
        };
        compare_known(step, dimension, &applicable, &mut errors);
    }
    errors
}

/// Whether a step joining these inputs demands this dimension be *known*.
///
/// The default [`required_pair`] table, read over the inputs that the
/// dimension applies to at all.
#[must_use]
pub fn requires(bindings: &[Binding], dimension: Dimension) -> bool {
    joined_on(bindings, dimension).is_some_and(|applicable| is_required(dimension, &applicable))
}

fn check_dimension(
    step: &str,
    bindings: &[Binding],
    dimension: Dimension,
    errors: &mut Vec<CheckError>,
) {
    let Some(applicable) = joined_on(bindings, dimension) else {
        return;
    };

    let (known, unknown): (Vec<&Binding>, Vec<&Binding>) = applicable
        .iter()
        .partition(|binding| binding.dataset.dimensions().is_known(dimension));

    if is_required(dimension, &applicable) && !unknown.is_empty() {
        match known.first() {
            // Each unknown input needs its own declaration.
            Some(pivot) => errors.extend(
                unknown
                    .iter()
                    .map(|binding| undeclared(step, dimension, binding, Some(pivot))),
            ),
            // Nothing is known, so one error covers them all.
            None => errors.push(undeclared_all(step, dimension, &unknown)),
        }
    }

    compare_known(step, dimension, &applicable, errors);
}

/// The inputs this dimension is actually joined on, if it is joined at all.
///
/// `None` when fewer than two inputs can speak to the dimension — nothing is
/// being compared, so nothing is required either.
fn joined_on(bindings: &[Binding], dimension: Dimension) -> Option<Vec<&Binding>> {
    let applicable: Vec<&Binding> = bindings
        .iter()
        .filter(|binding| binding.dataset.kind().applies(dimension))
        .collect();

    if applicable.len() < 2 {
        return None;
    }

    // Contig naming is derived from the reference, so checking the reference
    // subsumes it. Two errors for one root cause is noise. It is still
    // checked on its own when the reference is not known on both sides.
    if dimension == Dimension::ContigNaming
        && applicable
            .iter()
            .filter(|binding| binding.dataset.dimensions().reference.is_known())
            .count()
            >= 2
    {
        return None;
    }

    Some(applicable)
}

fn compare_known(
    step: &str,
    dimension: Dimension,
    applicable: &[&Binding],
    errors: &mut Vec<CheckError>,
) {
    let known: Vec<&Binding> = applicable
        .iter()
        .copied()
        .filter(|binding| binding.dataset.dimensions().is_known(dimension))
        .collect();

    if let Some((pivot, rest)) = known.split_first() {
        errors.extend(
            rest.iter()
                .filter_map(|other| compare(step, dimension, pivot, other)),
        );
    }
}

/// Whether a dimension must be known, not merely consistent.
fn is_required(dimension: Dimension, applicable: &[&Binding]) -> bool {
    applicable.iter().enumerate().any(|(index, left)| {
        applicable[index + 1..].iter().any(|right| {
            required_pair(left.dataset.kind(), right.dataset.kind()).contains(&dimension)
        })
    })
}

/// The dimensions that must be *known* when a step joins these two kinds.
///
/// Deliberately narrow. Organism, for instance, is absent from every pair
/// involving a reference: reference identity already implies it, so demanding
/// a declaration would add a hurdle without adding safety. Quality encoding
/// is absent too, because `web/content/reference/dimensions.mdx` gives no way
/// to declare it — requiring something a user cannot supply would be an
/// unsatisfiable check.
#[must_use]
pub const fn required_pair(left: DataKind, right: DataKind) -> &'static [Dimension] {
    use DataKind as K;

    const REF: &[Dimension] = &[Dimension::Reference];
    const COORDINATES: &[Dimension] = &[Dimension::Coordinates];
    const REF_AND_SAMPLES: &[Dimension] = &[Dimension::Reference, Dimension::SampleSet];
    const SAMPLES: &[Dimension] = &[Dimension::SampleSet];
    const SAMPLES_AND_PAIRING: &[Dimension] = &[Dimension::SampleSet, Dimension::Pairing];
    const IDENTIFIERS: &[Dimension] = &[Dimension::IdentifierNamespace];
    const NONE: &[Dimension] = &[];

    match (left, right) {
        // A resistance report is the end of the line; it joins nothing.
        (K::ResistanceReport, _) | (_, K::ResistanceReport) => NONE,

        // Two lists of named entities must each say which space they are
        // named in -- whether ranked or not, and in either combination.
        // Unlike organism, this is a demand a user can always meet --
        // declare it, or let the loader read it off the identifiers -- and
        // unknown on both sides is the false agreement the dimension
        // exists to break: two lists that say nothing would join happily
        // and overlap in nothing.
        //
        // The mixed pair is written out rather than left to the catch-all
        // below, because scoring a ranking against a curated truth set is
        // the commonest reason to join these two kinds at all, and the
        // catch-all would have quietly demanded nothing of it.
        (K::EntitySet | K::RankedList, K::EntitySet | K::RankedList) => IDENTIFIERS,
        // A named list and a sequence file share only organism, which is
        // demanded nowhere, so there is nothing to insist on.
        (K::EntitySet | K::RankedList, _) | (_, K::EntitySet | K::RankedList) => NONE,

        // Reads carry no reference yet -- introducing one is what `align`
        // does -- so a reference tells us nothing about them.
        (K::Reads, K::Reference) | (K::Reference, K::Reads) => NONE,
        // Unaligned reads and genomic intervals share nothing at all.
        (K::Reads, K::Intervals) | (K::Intervals, K::Reads) => NONE,
        (K::Reads, K::Reads) => SAMPLES_AND_PAIRING,
        (K::Reads, _) | (_, K::Reads) => SAMPLES,

        // Coordinates, and only coordinates. Both sides always know their own
        // convention, because it is a property of the format -- so this is a
        // demand the user can always satisfy, and it is the one mismatch that
        // no checksum comparison would catch.
        //
        // `reference` is deliberately *not* required here. An interval file's
        // contigs come from its records, so an empty one knows none, and
        // there is no way for a user to declare them. Requiring something
        // nobody can supply is an unsatisfiable check, and the catalogue
        // already treats that as worse than a missed one. It is still
        // *checked* whenever both sides state it, which is the normal case.
        (K::Intervals, _) | (_, K::Intervals) => COORDINATES,

        (K::Reference, _) | (_, K::Reference) => REF,

        (K::Alignment, K::Alignment) => REF,
        (K::Alignment, K::Variants) | (K::Variants, K::Alignment) => REF_AND_SAMPLES,
        (K::Variants, K::Variants) => REF_AND_SAMPLES,
    }
}

// --------------------------------------------------------- comparison --

fn compare(
    step: &str,
    dimension: Dimension,
    left: &Binding,
    right: &Binding,
) -> Option<CheckError> {
    if dimension == Dimension::Reference {
        return compare_reference(step, left, right);
    }

    let (a, b) = (left.dataset.dimensions(), right.dataset.dimensions());
    let same = match dimension {
        Dimension::Organism => a.organism.value() == b.organism.value(),
        Dimension::ContigNaming => a.contig_naming.value() == b.contig_naming.value(),
        Dimension::Coordinates => a.coordinates.value() == b.coordinates.value(),
        Dimension::SampleSet => a.sample_set.value() == b.sample_set.value(),
        Dimension::Pairing => a.pairing.value() == b.pairing.value(),
        Dimension::QualityEncoding => a.quality_encoding.value() == b.quality_encoding.value(),
        Dimension::Release => a.release.value() == b.release.value(),
        Dimension::Subject => a.subject.value() == b.subject.value(),
        Dimension::SourceSet => a.source_set.value() == b.source_set.value(),
        Dimension::IdentifierNamespace => {
            a.identifier_namespace.value() == b.identifier_namespace.value()
        }
        Dimension::Reference => unreachable(),
    };
    if same {
        return None;
    }

    let kind = if dimension == Dimension::ContigNaming {
        ErrorKind::ContigNaming
    } else {
        ErrorKind::Dimension
    };

    // Same members, different order, is worth saying out loud: the fix is a
    // reordering, not a different sample set.
    let detail = match dimension {
        Dimension::SampleSet => {
            let (Some(x), Some(y)) = (a.sample_set.value(), b.sample_set.value()) else {
                return None;
            };
            x.same_members(y)
                .then(|| "The same samples appear in both, in a different order.".to_owned())
        }
        // The lines above already state both sets in full; what they cannot
        // do is point at the member that makes the sets differ. One arm
        // serves both dimensions because the comparison rule is the same.
        Dimension::Organism | Dimension::SourceSet => {
            let (Some(x), Some(y)) = (name_set(a, dimension), name_set(b, dimension)) else {
                return None;
            };
            let clauses: Vec<String> = [(left, x.difference(y)), (right, y.difference(x))]
                .into_iter()
                .filter(|(_, extra)| !extra.is_empty())
                .map(|(binding, extra)| {
                    format!("{} also lists {}", binding.name, english_list(&extra))
                })
                .collect();
            // Two sets that compared unequal always differ somewhere, so an
            // empty list is not reachable. If it ever happened, the error
            // still stands on its two value lines; leaving the detail off is
            // more honest than inventing a clause, and a panic would turn a
            // reported disagreement into a crash.
            (!clauses.is_empty()).then(|| format!("{}.", clauses.join(", and ")))
        }
        _ => None,
    };

    Some(CheckError {
        kind,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(dimension),
        lines: vec![
            value_line(left, dimension, None),
            value_line(right, dimension, None),
        ],
        detail,
        conclusion: dimension.leading_question().to_owned(),
    })
}

/// The [`NameSet`] a dimension holds, for the dimensions that hold one.
///
/// A lookup that can say "none" rather than a catch-all arm: a dimension
/// added to the set-difference detail but not here yields no detail, which
/// is a missing sentence, instead of quietly reading another dimension's
/// value, which would be a wrong one.
fn name_set(dimensions: &Dimensions, dimension: Dimension) -> Option<&NameSet> {
    match dimension {
        Dimension::Organism => dimensions.organism.value(),
        Dimension::SourceSet => dimensions.source_set.value(),
        Dimension::Reference
        | Dimension::ContigNaming
        | Dimension::Coordinates
        | Dimension::SampleSet
        | Dimension::Pairing
        | Dimension::QualityEncoding
        | Dimension::Release
        | Dimension::Subject
        // A namespace is a `Label`, not a `NameSet`: there is no set
        // difference to point at, and the two value lines say everything
        // there is to say about one label against another.
        | Dimension::IdentifierNamespace => None,
    }
}

fn compare_reference(step: &str, left: &Binding, right: &Binding) -> Option<CheckError> {
    let a = left.dataset.dimensions().reference.value()?;
    let b = right.dataset.dimensions().reference.value()?;

    // Orientation follows the *kinds*, not the argument order: data is
    // checked against a reference, never the other way round.
    let (data, reference, relation) = match (left.dataset.kind(), right.dataset.kind()) {
        (DataKind::Reference, DataKind::Reference) => (a, b, Relation::Identical),
        (DataKind::Reference, _) => (b, a, Relation::DataWithin),
        (_, DataKind::Reference) => (a, b, Relation::DataWithin),
        _ => (a, b, Relation::Compatible),
    };

    match relate(data, reference, relation) {
        Verdict::Agree => None,

        Verdict::NamingDiffers {
            left: x,
            right: y,
            example,
        } => Some(CheckError {
            kind: ErrorKind::ContigNaming,
            step: step.to_owned(),
            node: None,
            scope: None,
            dimension: Some(Dimension::ContigNaming),
            // Rendered from each binding's own naming dimension, so the
            // lines always correspond to the bindings as listed.
            lines: vec![naming_line(left), naming_line(right)],
            detail: None,
            conclusion: naming_conclusion(x, y, &example),
        }),

        Verdict::OrderDiffers { .. } => Some(CheckError {
            kind: ErrorKind::ContigOrder,
            step: step.to_owned(),
            node: None,
            scope: None,
            dimension: Some(Dimension::Reference),
            lines: vec![order_line(left), order_line(right)],
            detail: None,
            conclusion: "The contigs agree but their order differs. Tools that require a \
                         matching sequence dictionary order, such as GATK, will reject this."
                .to_owned(),
        }),

        Verdict::MissingContig { name, from } => Some(reference_error(
            step,
            left,
            right,
            format!("Contig `{name}` is not in {from}."),
        )),

        Verdict::ContigConflict { name, detail } => Some(reference_error(
            step,
            left,
            right,
            format!("Contig `{name}`: {detail}."),
        )),
    }
}

fn reference_error(step: &str, left: &Binding, right: &Binding, detail: String) -> CheckError {
    // Two references can describe themselves identically -- two files both
    // labelled hg38, say. A message showing the same value twice cannot be
    // acted on, so fall back to fingerprints when that happens.
    let (a, b) = (left.dataset.dimensions(), right.dataset.dimensions());
    let suffixes = match (a.reference.value(), b.reference.value()) {
        (Some(x), Some(y)) if x.describe() == y.describe() => (
            Some(format!(" [fingerprint {}]", x.fingerprint())),
            Some(format!(" [fingerprint {}]", y.fingerprint())),
        ),
        _ => (None, None),
    };

    CheckError {
        kind: ErrorKind::Dimension,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(Dimension::Reference),
        lines: vec![
            value_line(left, Dimension::Reference, suffixes.0.as_deref()),
            value_line(right, Dimension::Reference, suffixes.1.as_deref()),
        ],
        detail: Some(detail),
        conclusion: Dimension::Reference.leading_question().to_owned(),
    }
}

fn naming_conclusion(
    left: ContigNaming,
    right: ContigNaming,
    example: &(String, String),
) -> String {
    if left == right && !example.0.is_empty() {
        // Same style, different spelling: `Chr1` against `chr1`. Naming the
        // style alone would say nothing useful, so show the names.
        return format!(
            "The contigs are otherwise identical, but the names differ (`{}` against `{}`). \
             Add a renaming step, or use one naming style throughout.",
            example.0, example.1
        );
    }
    Dimension::ContigNaming.leading_question().to_owned()
}

// ------------------------------------------------------------ rendering --

pub(crate) fn value_line(binding: &Binding, dimension: Dimension, suffix: Option<&str>) -> String {
    let dimensions = binding.dataset.dimensions();
    let value = dimensions
        .render(dimension)
        .unwrap_or_else(|| "unknown".to_owned());
    let source = dimensions
        .source(dimension)
        .map(crate::dim::Source::describe)
        .unwrap_or_else(|| "source unknown".to_owned());
    format!(
        "{}.{dimension} = {value}{} ({source})",
        binding.name,
        suffix.unwrap_or_default()
    )
}

fn naming_line(binding: &Binding) -> String {
    let dimensions = binding.dataset.dimensions();
    // Naming is derived from the reference, so fall back to it when the
    // dimension was not filled in separately.
    let value = dimensions
        .render(Dimension::ContigNaming)
        .or_else(|| {
            dimensions
                .reference
                .value()
                .map(|reference| reference.naming().describe().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let source = dimensions
        .source(Dimension::ContigNaming)
        .or_else(|| dimensions.source(Dimension::Reference))
        .map(crate::dim::Source::describe)
        .unwrap_or_else(|| "source unknown".to_owned());
    format!("{}.contig_naming = {value} ({source})", binding.name)
}

fn order_line(binding: &Binding) -> String {
    let dimensions = binding.dataset.dimensions();
    let order = dimensions
        .reference
        .value()
        .map(contig_order)
        .unwrap_or_else(|| "unknown".to_owned());
    let source = dimensions
        .source(Dimension::Reference)
        .map(crate::dim::Source::describe)
        .unwrap_or_else(|| "source unknown".to_owned());
    format!(
        "{}.reference lists contigs in the order {order} ({source})",
        binding.name
    )
}

/// The contig names in order, abbreviated so a whole-genome reference does
/// not print 3000 scaffolds into an error message.
fn contig_order(reference: &ReferenceId) -> String {
    const SHOWN: usize = 6;
    let contigs = reference.contigs();
    let listed: Vec<&str> = contigs
        .iter()
        .take(SHOWN)
        .map(|contig| contig.name.as_str())
        .collect();
    if contigs.len() > SHOWN {
        format!("{}, ... ({} in all)", listed.join(", "), contigs.len())
    } else {
        listed.join(", ")
    }
}

fn undeclared(
    step: &str,
    dimension: Dimension,
    binding: &Binding,
    known: Option<&&Binding>,
) -> CheckError {
    // The unknown side leads, because that is the one the user must change.
    let mut lines = vec![unknown_line(binding, dimension)];
    if let Some(other) = known {
        lines.push(value_line(other, dimension, None));
    }
    CheckError {
        kind: ErrorKind::Undeclared,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(dimension),
        lines,
        detail: None,
        conclusion: format!(
            "`{step}` needs {dimension} to agree. Declare it where you load the data."
        ),
    }
}

fn undeclared_all(step: &str, dimension: Dimension, unknown: &[&Binding]) -> CheckError {
    CheckError {
        kind: ErrorKind::Undeclared,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(dimension),
        lines: unknown
            .iter()
            .map(|binding| unknown_line(binding, dimension))
            .collect(),
        detail: None,
        conclusion: format!(
            "`{step}` needs {dimension} to agree, and nothing states it. \
             Declare it where you load the data."
        ),
    }
}

pub(crate) fn unknown_line(binding: &Binding, dimension: Dimension) -> String {
    let reason = binding
        .dataset
        .dimensions()
        .reason(dimension)
        .unwrap_or("not recorded");
    format!("{}.{dimension} is unknown ({reason})", binding.name)
}

/// The `Dimension::Reference` arm of [`compare`] is handled before the match.
///
/// Written as a diverging call rather than `unreachable!`, which the lint
/// configuration forbids alongside `unwrap` and `expect`.
const fn unreachable() -> bool {
    // A wrong answer here would mean silently skipping a reference check, so
    // return the value that reports a disagreement rather than hiding one.
    false
}

// ------------------------------------------------- errors the graph builds --

/// A step was handed a kind of data it does not declare.
///
/// Reported instead of the step's dimension errors, not alongside them: the
/// bindings are not what the step declared, so comparing their dimensions
/// would describe a pipeline the user never wrote.
#[must_use]
pub fn step_type_error(
    step: &str,
    param: &str,
    expected: DataKind,
    found: DataKind,
    origin: &str,
) -> CheckError {
    CheckError {
        kind: ErrorKind::StepType,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: None,
        lines: vec![
            format!("`{step}` declares {param}: {}", expected.name()),
            format!("{param} was given {} (from {origin})", found.name()),
        ],
        detail: None,
        conclusion: format!(
            "These must match. Did you pass the wrong value, or annotate `{step}` \
             with the wrong type?"
        ),
    }
}

/// A dimension a step requires is unknown, attributed to where the gap began.
///
/// `web/content/reference/dimensions.mdx` allows an unknown dimension unless
/// a downstream step requires it, so this error exists only because some step
/// does. It names the step that requires it and the data that has to declare
/// it, which for a value that travelled through the graph are two different
/// places — pointing only at the step would send the user to a file they
/// cannot fix.
#[must_use]
pub fn undeclared_at(
    step: &str,
    dimension: Dimension,
    origin: &str,
    reason: &str,
    through: &[&str],
) -> CheckError {
    let detail = match through {
        [] => None,
        steps => Some(format!(
            "It reaches `{step}` through {}.",
            list_steps(steps)
        )),
    };
    CheckError {
        kind: ErrorKind::Undeclared,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(dimension),
        lines: vec![format!("{dimension} is unknown in {origin} ({reason})")],
        detail,
        conclusion: format!(
            "`{step}` needs {dimension} to agree. Declare it where you load the data."
        ),
    }
}

/// A reference that is not numbered along the frame a step requires.
///
/// Built here rather than where the frame is known, so that this reads like
/// every other finding: both values, where each came from, then what to do.
/// The engine's own side has no file to have come from, so it says so.
#[must_use]
pub fn reference_frame(
    step: &str,
    binding: &Binding,
    expected: &str,
    detail: String,
    conclusion: String,
) -> CheckError {
    CheckError {
        kind: ErrorKind::ReferenceFrame,
        step: step.to_owned(),
        node: None,
        scope: None,
        dimension: Some(Dimension::Reference),
        lines: vec![
            value_line(binding, Dimension::Reference, None),
            format!("{step} reads positions in {expected} (built into this engine)"),
        ],
        detail: Some(detail),
        conclusion,
    }
}

/// `step `a``, or `steps `a` and `b``, or `steps `a`, `b` and `c``.
fn list_steps(steps: &[&str]) -> String {
    let quoted: Vec<String> = steps.iter().map(|step| format!("`{step}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => format!("step {last}"),
        Some((last, rest)) => format!("steps {} and {last}", rest.join(", ")),
    }
}

/// One line summarising a check, for a command-line report.
///
/// The wording lives here with the errors themselves, so the Rust tests own
/// it and a caller in any language reports a check the same way.
///
/// The steps counted are the *places* that failed, not the names they share.
/// Under per-sample fan-out one written step is many steps, and a cohort-wide
/// failure reported as a single localised one is the opposite of what a user
/// needs in order to decide whether to re-run a sample or rethink the
/// pipeline.
#[must_use]
pub fn summarise(errors: &[CheckError]) -> String {
    if errors.is_empty() {
        return "no problems found.".to_owned();
    }

    let mut places: Vec<String> = errors.iter().map(place).collect();
    places.sort_unstable();
    places.dedup();

    format!(
        "{} {} in {} {}.",
        errors.len(),
        plural(errors.len(), "error", "errors"),
        places.len(),
        plural(places.len(), "step", "steps")
    )
}

/// What tells the step an error came from apart from any other.
///
/// A graph stamps a node; a single step checked on its own has only its name.
fn place(error: &CheckError) -> String {
    match error.node {
        Some(node) => format!("#{node}"),
        None => format!("`{}`", error.step),
    }
}

/// The whole report: each distinct problem once, then the summary.
///
/// One problem across four hundred samples is one thing to understand and
/// four hundred paragraphs to read, so identical problems are stated once and
/// every sample they affect is named on a line of its own. Named, not
/// counted: a user who has to expand something to find out which samples
/// failed has been given a worse report, not a shorter one.
///
/// Errors that cannot name themselves are never collapsed. Hiding one behind
/// another identical-looking error would leave a user with no way to tell
/// them apart, which is the bug this exists to avoid rather than a smaller
/// version of it.
#[must_use]
pub fn render(errors: &[CheckError]) -> String {
    if errors.is_empty() {
        return summarise(errors);
    }

    let mut groups: Vec<Group<'_>> = Vec::new();
    for error in errors {
        let key = error.scope.as_ref().map(|_| write_error(error, false));
        let existing = key.as_deref().and_then(|key| {
            groups
                .iter()
                .position(|group| group.key.as_deref() == Some(key))
        });
        match existing {
            Some(at) => groups[at].rest.push(error),
            None => groups.push(Group {
                key,
                first: error,
                rest: Vec::new(),
            }),
        }
    }

    let mut paragraphs: Vec<String> = groups.iter().map(Group::render).collect();
    paragraphs.push(summarise(errors));
    paragraphs.join("\n\n")
}

/// One problem, and the other places it was found.
struct Group<'a> {
    /// The problem as text without its scope, or `None` when it has none and
    /// therefore stands alone.
    key: Option<String>,
    first: &'a CheckError,
    rest: Vec<&'a CheckError>,
}

impl Group<'_> {
    fn render(&self) -> String {
        let mut out = self.first.to_string();
        let named: Vec<&str> = self
            .rest
            .iter()
            .filter_map(|error| error.scope.as_ref())
            .map(|scope| scope.value.as_str())
            .collect();

        if let Some(scope) = self.first.scope.as_ref()
            && !named.is_empty()
        {
            let kind = if named.len() == 1 {
                scope.kind.to_owned()
            } else {
                scope.kinds()
            };
            out.push_str(&format!(
                "\n  The same problem affects {} other {kind}: {}.",
                named.len(),
                named.join(", ")
            ));
        }
        out
    }
}

const fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}
