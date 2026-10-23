//! Describing one dataset in words, for a surface that only looks.
//!
//! `ligase inspect` asks a smaller question than `check` does: not "will this
//! pipeline work" but "what does Ligase think this file is". There is no
//! graph, nothing to run and nothing to compare — just one file and what the
//! loader made of it. That makes this the first thing about Ligase most
//! readers will ever see, and the reason the wording lives here rather than
//! in the command that prints it: every surface should say the same thing
//! about the same file, the way [`crate::types::Dataset::notes`] already
//! requires.
//!
//! # The same line, twice
//!
//! A description line is rendered through the helpers [`crate::check`]
//! renders a diagnostic's value lines with, so the two are one line defined
//! once. The diagnostic a reader gets after a set of descriptions is then
//! visibly a *selection* of the lines above it, rather than a second account
//! of the same file in different words.
//!
//! # Three states, not two
//!
//! A dimension is shown with its value and its source, or with the engine's
//! reason for not having one, or not at all — and the third case is the one
//! worth being careful about. [`Dimensions::reason`](crate::dim::Dimensions::reason)
//! reads its field and nothing else, so it answers for a dimension that does
//! not apply to the kind as readily as for one that does. Asking it without
//! asking [`DataKind::applies`](crate::types::DataKind::applies) first would
//! report a FASTA as *missing* a quality encoding, which is not a gap a user
//! can close: it is a category error, and it would send them looking for a
//! declaration that does not exist.
//!
//! The dimensions that do not apply are still named, once, on a line that
//! says they do not. Dropping them silently would leave a reader unable to
//! tell "this file does not say" from "this question does not arise", which
//! is the distinction the dimension matrix exists to draw.

use crate::check::{Binding, unknown_line, value_line};
use crate::dim::{Dimension, display_path, english_list};
use crate::types::Dataset;

/// Describe one named dataset: what it is, and what is known about it.
///
/// The name is the one a diagnostic would use for the same dataset — the
/// `ref` in `ref.organism` — so that a description and an error about the
/// same file address it the same way.
///
/// Returns the block as text, without a trailing newline, for a caller to
/// place among whatever else it is printing.
#[must_use]
pub fn describe(binding: &Binding) -> String {
    let kind = binding.dataset.kind();
    let dimensions = binding.dataset.dimensions();

    let where_from = binding
        .dataset
        .path()
        .map_or_else(|| "<in memory>".to_owned(), display_path);
    let mut lines = vec![format!("{}  {where_from}", kind.name())];

    let applicable: Vec<Dimension> = Dimension::ALL
        .into_iter()
        .filter(|dimension| kind.applies(*dimension))
        .collect();

    // Known first, then the gaps. A reader scanning for what Ligase worked
    // out should not have to step over what it could not.
    for dimension in applicable
        .iter()
        .filter(|dimension| dimensions.is_known(**dimension))
    {
        lines.push(format!("  {}", value_line(binding, *dimension, None)));
    }
    for dimension in applicable
        .iter()
        .filter(|dimension| !dimensions.is_known(**dimension))
    {
        lines.push(format!("  {}", unknown_line(binding, *dimension)));
    }

    let inapplicable: Vec<&str> = Dimension::ALL
        .into_iter()
        .filter(|dimension| !kind.applies(*dimension))
        .map(Dimension::name)
        .collect();
    if !inapplicable.is_empty() {
        lines.push(format!(
            "  {} do not apply to {} {}.",
            english_list(&inapplicable),
            article(kind.name()),
            kind.name()
        ));
    }

    // What loading noticed. Not a failure and not a dimension: something
    // true of the file that a reader cannot act on without being told, which
    // is the whole reason for looking at a file you already have.
    for note in binding.dataset.notes() {
        lines.push(format!("  noticed  {note}"));
    }

    lines.join("\n")
}

/// `a` or `an`, for the one place a kind's name follows an article.
///
/// Only the kind names pass through here, so a vowel test is enough: there
/// is no `Hour`-shaped name among them, and there will not be one without
/// this line being read again.
const fn article(name: &str) -> &'static str {
    match name.as_bytes() {
        [b'A' | b'E' | b'I' | b'O' | b'U', ..] => "an",
        _ => "a",
    }
}

/// The dimensions two or more of these datasets state a value for.
///
/// What a verdict over several files actually covers. "These files agree" is
/// a claim a reader cannot size without it: a FASTQ and a BED share no
/// dimension either of them records, so they agree in the way two sentences
/// in different languages agree, and saying so without qualification would be
/// false comfort.
///
/// A dimension is listed when at least two of the datasets both apply it and
/// know it, which is exactly the condition under which
/// [`check_step`](crate::check::check_step) compares values. Contig naming is
/// compared *through* the reference relation when both sides know a reference
/// — `check` subsumes the one into the other to avoid reporting a single
/// cause twice — so it is compared either way and is listed either way.
///
/// In [`Dimension::ALL`] order, which is the order a report reads in.
#[must_use]
pub fn comparable(datasets: &[Dataset]) -> Vec<&'static str> {
    Dimension::ALL
        .into_iter()
        .filter(|dimension| {
            datasets
                .iter()
                .filter(|dataset| {
                    dataset.kind().applies(*dimension) && dataset.dimensions().is_known(*dimension)
                })
                .count()
                >= 2
        })
        .map(Dimension::name)
        .collect()
}
