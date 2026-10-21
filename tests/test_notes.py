"""D32: what a step noticed about its own work reaching the person.

The Rust tests own the mechanism — that a body's notes are carried, stored
against the step's key and said again when the step is served from the
cache. These own the Python surface of it: that `lg.hivdb.interpret` hands
its reading's hedges back rather than dropping them, that they arrive on
`pipe.run()`'s report and in the text the command line prints, and that a
body which notices nothing stays silent.

The hedges matter more than the mechanism. A resistance report written
without coverage looks exactly like one written with full coverage — the
`unread` column is empty either way — so if this chain breaks anywhere, a
run that took every uncalled position to be reference says so nowhere, and
a susceptible result manufactured out of a dropout reads as a result.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

import ligase as lg
from ligase.types import Reference, ResistanceReport, Variants
from tests.conftest import fixture

if TYPE_CHECKING:
    from pathlib import Path

#: One call against HXB2, which is all a reading needs to have something to
#: do. The contig is named as HXB2 names it, so the reference check passes.
CALLS = (
    "##fileformat=VCFv4.3\n"
    "##contig=<ID=K03455.1,length=9719>\n"
    "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample_a\n"
    "K03455.1\t2549\t.\tA\tG\t30\tPASS\t.\tGT\t0/1\n"
)


@lg.step(tool="hivdb")
def resistance(v: Variants, ref: Reference) -> ResistanceReport:
    """Interpret without an alignment, which is the assumption at issue."""
    return lg.hivdb.interpret(v, ref)


def _write_nothing_much(out: Path) -> None:
    """Write an output, and hand nothing back."""
    out.write_text("##fileformat=VCFv4.3\n")


@lg.step
def silent(ref: Reference) -> Variants:
    """A body that does its work and has nothing to say about it."""
    return lg.write(_write_nothing_much)


@lg.step
def confused(ref: Reference) -> Variants:
    """A body that hands back whatever its last expression was.

    `Path.write_text` returns how many characters it wrote, and
    `lg.write(lambda out: out.write_text(...))` is the natural one-liner,
    so this is the mistake a user makes rather than an invented one.

    `mypy --strict` refuses this line, which is the first line of defence
    and the reason for the `ignore` below. The message it earns at run time
    is for a pipeline nobody type-checks.
    """
    return lg.write(
        lambda out: out.write_text(  # type: ignore[arg-type, return-value]
            "##fileformat=VCFv4.3\n"
        )
    )


@lg.step
def inventive(ref: Reference) -> Variants:
    """A body that claims to have noticed something no note names."""

    def produce(out: Path) -> list[tuple[str, str]]:
        out.write_text("##fileformat=VCFv4.3\n")
        return [("something-i-made-up", "x")]

    return lg.write(produce)


def calls(tmp_path: Path) -> Variants:
    """The calls, written where the test can hand them to a pipeline."""
    path = tmp_path / "calls.vcf"
    path.write_text(CALLS)
    return lg.variants(str(path))


def notes(report: lg.RunReport, step: str = "resistance") -> tuple[str, ...]:
    """Everything noticed about one step, over however many nodes it has."""
    return tuple(note for run in report.steps if run.step == step for note in run.notes)


def barely_read(tmp_path: Path) -> tuple[Variants, Reference]:
    """Whole-genome K03455 with two thirds of integrase unsequenced.

    `N` is what a sequence that did not reach a region actually holds, and
    it is a different thing from a sequence that agrees with the reference.
    Ninety-two of integrase's 288 codons is the 32% SAWA saw in the field.
    """
    bases = ["A"] * 9719
    for position in range(4230 + 92 * 3, 5093 + 1):
        bases[position - 1] = "N"
    fasta = tmp_path / "whole.fasta"
    fasta.write_text(">K03455.1 whole\n" + "".join(bases) + "\n")
    return (
        calls(tmp_path),
        lg.reference(str(fasta), organism="HIV-1", build="HXB2"),
    )


def fragment(tmp_path: Path) -> tuple[Variants, Reference]:
    """A hundred bases called HXB2, which no HIV-1 gene fits inside.

    With calls of its own, declaring the same hundred bases: a VCF whose
    contig lines disagree with the FASTA is refused before anything runs,
    which is a different test.
    """
    fasta = tmp_path / "fragment.fasta"
    fasta.write_text(">K03455.1 a fragment\n" + "ACGT" * 25 + "\n")
    vcf = tmp_path / "fragment.vcf"
    vcf.write_text(
        CALLS.replace("length=9719", "length=100").replace("\t2549\t", "\t5\t")
    )
    return (
        lg.variants(str(vcf)),
        lg.reference(str(fasta), organism="HIV-1", build="HXB2"),
    )


# ------------------------------------------- what a reading says it did --


def test_a_reading_without_coverage_says_so(hxb2: str, tmp_path: Path) -> None:
    """The assumption D29 exists to stop being silent.

    `bcftools call -mv` writes only the sites it called a variant at, so a
    position missing from the VCF is ambiguous between agreeing with HXB2
    and nothing having aligned there. Without an alignment the reading
    takes the first, which manufactures susceptible results out of
    dropouts, and the report it writes cannot say that it did.
    """
    reference = lg.reference(hxb2, organism="HIV-1", build="HXB2")
    pipe = lg.Pipeline(resistance(calls(tmp_path), reference))

    report = pipe.run(cache=tmp_path / "cache")

    assert notes(report) == (
        "no alignment was supplied, so coverage is unknown, no codon can be "
        "phased, and every position the caller did not report is taken to be "
        "reference",
    )
    # And in the text the command line prints, with what to do about it.
    assert "noticed" in report.text
    assert "coverage is unknown" in report.text
    assert "coverage=aln" in report.text


def test_a_cached_reading_still_says_what_it_noticed(hxb2: str, tmp_path: Path) -> None:
    """A hedge that appears only on the first run is worse than none.

    The body is not rerun for a cache hit, so there is nothing to ask: the
    note either was stored against the step's key or has gone quiet. A
    clinician reading the second run's report would have no way to tell
    that the first one hedged.
    """
    reference = lg.reference(hxb2, organism="HIV-1", build="HXB2")
    cache = tmp_path / "cache"
    pipe = lg.Pipeline(resistance(calls(tmp_path), reference))
    first = pipe.run(cache=cache)
    assert [run.outcome for run in first.steps] == ["ran"]

    again = lg.Pipeline(resistance(calls(tmp_path), reference)).run(cache=cache)

    assert [run.outcome for run in again.steps] == ["cached"]
    assert notes(again) == notes(first)
    assert "coverage is unknown" in again.text


def test_a_reading_says_which_gene_it_barely_read(tmp_path: Path) -> None:
    """A gene mostly unsequenced is left out rather than called susceptible.

    A gene read in part refutes nothing and shows nothing, so every drug
    scored against it lands at zero, which this algorithm's own ranges read
    as susceptible. The gene is left out of the report instead, and an
    absent drug class means no data rather than a negative result — which
    is only true if someone is told.
    """
    variants, reference = barely_read(tmp_path)
    pipe = lg.Pipeline(resistance(variants, reference))

    report = pipe.run(cache=tmp_path / "cache")

    said = notes(report)
    assert any(
        note.startswith("only 92 of IN's 288 codons were read") for note in said
    ), said
    assert "gene_coverage=" in report.text


def test_a_reference_too_short_for_the_genes_is_refused(tmp_path: Path) -> None:
    """Which is better than the note this used to be.

    The gene windows are positions in whole-genome K03455, and a reference
    they do not fit inside is not that genome. Noting it gene by gene left
    the dangerous case silent: on a pol-relative reference the windows for
    the genes that *do* fit land on the wrong genes, and protease was read
    out of integrase without a word.
    """
    short, reference = fragment(tmp_path)
    pipe = lg.Pipeline(resistance(short, reference))

    with pytest.raises(lg.ReferenceFrameError) as raised:
        pipe.run(cache=tmp_path / "cache")

    message = str(raised.value)
    assert "100 bases" in message
    assert "gene windows reach base 5093" in message


# ----------------------------------------------- and when there is none --


def test_a_step_that_noticed_nothing_says_nothing(tmp_path: Path) -> None:
    """The common case has to stay quiet.

    A report that remarks on every run is a report a reader stops reading,
    which would cost exactly the notes that matter.
    """
    reference = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")

    report = lg.Pipeline(silent(reference)).run(cache=tmp_path)

    assert notes(report, "silent") == ()
    assert "noticed" not in report.text


def test_a_body_that_hands_back_something_else_is_told_what_to_return(
    tmp_path: Path,
) -> None:
    reference = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")

    with pytest.raises(lg.RunError) as raised:
        lg.Pipeline(confused(reference)).run(cache=tmp_path)

    assert "A body returns `None`" in str(raised.value)


def test_a_body_that_invents_a_note_is_a_failure(tmp_path: Path) -> None:
    """A code this engine has no note for is a mistake, said out loud.

    Dropping it quietly is the silence this whole path exists to end: a
    body that misspells a code would go on believing it had hedged.
    """
    reference = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")

    with pytest.raises(lg.RunError) as raised:
        lg.Pipeline(inventive(reference)).run(cache=tmp_path)

    assert "something-i-made-up" in str(raised.value)
    assert "not a note this engine has" in str(raised.value)
