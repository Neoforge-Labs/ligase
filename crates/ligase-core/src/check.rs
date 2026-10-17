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
//! `docs/dimensions.md` says unknown dimensions are allowed only if no
//! downstream step requires them. Without the graph (M2) there is no
//! downstream yet, so "required" comes from the kinds being joined. The split
//! is what lets the golden example work: `lg.samples()` takes no organism,
//! and `align` is the step that *introduces* the reference, so demanding a
//! declared organism on the FASTQs would break a documented API for no
//! safety gain.
//!
//! Erring toward demands would be the worse failure. A checker that cries
//! wolf gets bypassed, and a bypassed checker catches nothing.

use std::error::Error;
use std::fmt;

use crate::dim::{ContigNaming, Dimension, ReferenceId, Relation, Verdict, relate};
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
    /// A dimension the step requires is unknown.
    Undeclared,
}

impl ErrorKind {
    /// The heading this kind of error leads with.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Dimension => "DimensionError",
            Self::ContigNaming => "ContigNamingError",
            Self::ContigOrder => "ContigOrderError",
            Self::Undeclared => "UndeclaredDimensionError",
        }
    }

    /// The name used in the Python API to pick an exception class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dimension => "dimension",
            Self::ContigNaming => "contig_naming",
            Self::ContigOrder => "contig_order",
            Self::Undeclared => "undeclared",
        }
    }
}

/// One disagreement found by [`check_step`].
///
/// The rendered message names the step, both conflicting values, where each
/// value came from, and asks a leading question — the shape specified at the
/// end of `docs/golden-example.md`.
#[derive(Clone, Debug)]
pub struct CheckError {
    kind: ErrorKind,
    step: String,
    dimension: Dimension,
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

    /// The dimension that disagrees.
    #[must_use]
    pub const fn dimension(&self) -> Dimension {
        self.dimension
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} in step `{}`:", self.kind.heading(), self.step)?;
        for line in &self.lines {
            write!(f, "\n  {line}")?;
        }
        if let Some(detail) = &self.detail {
            write!(f, "\n  {detail}")?;
        }
        write!(f, "\n  {}", self.conclusion)
    }
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

fn check_dimension(
    step: &str,
    bindings: &[Binding],
    dimension: Dimension,
    errors: &mut Vec<CheckError>,
) {
    let applicable: Vec<&Binding> = bindings
        .iter()
        .filter(|binding| binding.dataset.kind().applies(dimension))
        .collect();

    // Nothing is being joined on this dimension, so nothing is required.
    if applicable.len() < 2 {
        return;
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
        return;
    }

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
/// is absent too, because `docs/dimensions.md` gives no way to declare it —
/// requiring something a user cannot supply would be an unsatisfiable check.
#[must_use]
pub const fn required_pair(left: DataKind, right: DataKind) -> &'static [Dimension] {
    use DataKind as K;

    const REF: &[Dimension] = &[Dimension::Reference];
    const COORDINATES: &[Dimension] = &[Dimension::Coordinates];
    const REF_AND_SAMPLES: &[Dimension] = &[Dimension::Reference, Dimension::SampleSet];
    const SAMPLES: &[Dimension] = &[Dimension::SampleSet];
    const SAMPLES_AND_PAIRING: &[Dimension] = &[Dimension::SampleSet, Dimension::Pairing];
    const NONE: &[Dimension] = &[];

    match (left, right) {
        // A resistance report is the end of the line; it joins nothing.
        (K::ResistanceReport, _) | (_, K::ResistanceReport) => NONE,

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
        _ => None,
    };

    Some(CheckError {
        kind,
        step: step.to_owned(),
        dimension,
        lines: vec![
            value_line(left, dimension, None),
            value_line(right, dimension, None),
        ],
        detail,
        conclusion: dimension.leading_question().to_owned(),
    })
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
            dimension: Dimension::ContigNaming,
            // Rendered from each binding's own naming dimension, so the
            // lines always correspond to the bindings as listed.
            lines: vec![naming_line(left), naming_line(right)],
            detail: None,
            conclusion: naming_conclusion(x, y, &example),
        }),

        Verdict::OrderDiffers { .. } => Some(CheckError {
            kind: ErrorKind::ContigOrder,
            step: step.to_owned(),
            dimension: Dimension::Reference,
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
        dimension: Dimension::Reference,
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

fn value_line(binding: &Binding, dimension: Dimension, suffix: Option<&str>) -> String {
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
        dimension,
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
        dimension,
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

fn unknown_line(binding: &Binding, dimension: Dimension) -> String {
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
