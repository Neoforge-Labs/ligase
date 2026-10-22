"""A reference that is not in the frame a step's coordinates are stated in.

`lg.hivdb.interpret` holds the HIV-1 gene windows as positions in HXB2
K03455, counted from the first base of the genome. SAWA, interpreting the
same virus from pol-relative sequence -- K03455 2085..5096, so every
position 2084 lower -- found what that used to cost: the window named
protease landed in integrase, and the report came back with
protease-inhibitor resistance at a clinical level, with no error and no
note.

Which frame a reference is numbered along cannot be read out of a FASTA, so
a whole genome is recognised by its length and anything else is declared.
These tests are about both halves: that the declaration is honoured, and
that a reference nobody declared and nothing can vouch for is refused
rather than read.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from ligase.types import Reference, ResistanceReport, Variants

#: Where pol starts in K03455, 1-based.
POL = 2085

#: How long whole-genome K03455 is.
BASES = 9719


@lg.step(tool="hivdb")
def resistance(v: Variants, ref: Reference) -> ResistanceReport:
    """Interpret variants as drug resistance."""
    return lg.hivdb.interpret(v, ref)


def inputs(
    directory: Path,
    bases: int,
    *,
    pad: int = 0,
    frame: tuple[str, int] | None = None,
) -> tuple[Variants, Reference]:
    """A reference of `bases` bases and calls against it.

    The first `pad` bases are `N`, which is what the padding workaround
    puts in front of a pol-relative sequence. Nothing else about the
    pipeline disagrees, so a frame error is the only thing a check can
    find.
    """
    fasta = directory / "reference.fasta"
    fasta.write_text(f">K03455 HIV-1\n{'N' * pad}{'A' * (bases - pad)}\n")

    vcf = directory / "calls.vcf"
    vcf.write_text(
        "##fileformat=VCFv4.2\n"
        f"##contig=<ID=K03455,length={bases}>\n"
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample\n"
    )

    return (
        lg.variants(str(vcf), organism="HIV-1"),
        lg.reference(str(fasta), organism="HIV-1", frame=frame),
    )


def frame_error(variants: Variants, reference: Reference) -> lg.ReferenceFrameError:
    """The frame error a one-step pipeline reports, and nothing else."""
    pipe = lg.Pipeline(resistance(variants, reference))
    with pytest.raises(lg.ReferenceFrameError) as caught:
        pipe.check()
    return caught.value


def test_a_pol_relative_reference_is_refused_before_anything_runs(
    tmp_path: Path,
) -> None:
    error = frame_error(*inputs(tmp_path, 5096 - POL + 1))

    # Still a DimensionError, so code that wants any disagreement about
    # data catches the base class.
    assert isinstance(error, lg.DimensionError)
    assert error.dimension == "reference"
    assert error.step == "resistance"

    message = str(error)
    assert message.startswith("ReferenceFrameError in step `resistance`:")
    # Both sides, and where each came from.
    assert "ref.reference" in message
    assert "FASTA contigs" in message
    assert "built into this engine" in message
    assert "3012 bases" in message
    assert f"{BASES} bases" in message
    # And what to do.
    assert 'frame=("K03455", 1)' in message


def test_declaring_the_slice_honestly_is_still_refused(tmp_path: Path) -> None:
    # The declaration is true and the answer is still no: this engine reads
    # whole-genome coordinates and does not shift its windows, which is a
    # conversion and so out of v0.1's scope.
    message = str(frame_error(*inputs(tmp_path, 5096 - POL + 1, frame=("K03455", POL))))
    assert "base 2085 of K03455" in message
    assert "2084 lower" in message
    assert '"N" * 2084 + pol' in message


def test_the_padding_workaround_passes_the_check(tmp_path: Path) -> None:
    # SAWA's fix: put the sequence back where its own numbers mean what the
    # gene windows say, and say so.
    pipe = lg.Pipeline(
        resistance(*inputs(tmp_path, 5096, pad=POL - 1, frame=("K03455", 1)))
    )
    assert pipe.errors() == []


def test_a_whole_genome_reference_needs_no_declaration(tmp_path: Path) -> None:
    # 9,719 bases is K03455, or something deliberately numbered along it.
    # The demo declares nothing and keeps working.
    pipe = lg.Pipeline(resistance(*inputs(tmp_path, BASES)))
    assert pipe.errors() == []


def test_a_reference_in_between_is_asked_about_rather_than_guessed_at(
    tmp_path: Path,
) -> None:
    # Long enough to hold the windows, but not the whole frame. A slice of
    # K03455 and a genome of something else look identical from outside.
    message = str(frame_error(*inputs(tmp_path, 6000)))
    assert "6000 bases" in message
    assert "cannot tell from the file" in message
    assert "Declare it where you load the reference" in message


def test_a_declaration_that_is_not_true_is_caught_when_it_is_read(
    tmp_path: Path,
) -> None:
    """The reading is the floor under a declaration nobody can check.

    A declared frame is taken at its word -- there is nothing in a FASTA to
    check it against -- so a pol-relative reference declared as a whole
    genome passes `check()`. It does not survive being read: the windows
    reach base 5093 and the file does not, which the reading can see for
    itself.
    """
    variants, reference = inputs(tmp_path, 5096 - POL + 1, frame=("K03455", 1))
    pipe = lg.Pipeline(resistance(variants, reference))
    assert pipe.errors() == []

    recipe = lg.hivdb.interpret(variants, reference)
    with pytest.raises(lg.ReferenceFrameError) as caught:
        recipe.function(tmp_path / "report.tsv")  # type: ignore[attr-defined]

    error = caught.value
    # No step to name: a check that ran before this would have said which.
    assert error.step is None
    assert error.dimension == "reference"
    assert "3012 bases" in str(error)
    assert 'frame=("K03455", 1)' in str(error)
