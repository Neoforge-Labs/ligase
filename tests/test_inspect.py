"""M9 acceptance: `ligase inspect` on files you already have.

No pipeline, no decorator, no cache, nothing executed — the ten-second trial
on two files off a shared drive. So these tests check the two things that
makes it worth running: that one file's description names every dimension
Ligase worked out *and* why the rest are unknown, and that two files get the
same diagnostic a pipeline would have got, with a non-zero exit code so the
answer survives a shell pipeline.

The exit codes are load-bearing and are checked for their own sake. 1 means
the files disagree; 2 means a file could not be read, so there is no verdict
at all. A pipeline that cannot tell those apart is a pipeline that treats an
unreadable file as a clean bill of health.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from ligase.cli import main
from tests.conftest import fixture


def test_two_disagreeing_files_print_the_diagnostic_and_exit_non_zero(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(["inspect", fixture("tiny_bare.fasta"), fixture("tiny_prefixed.sam")])

    out = capsys.readouterr().out
    # Non-zero, so `ligase inspect a b && do_the_analysis` does the right
    # thing without anyone having to parse the output.
    assert code == 1, out
    assert "ContigNamingError" in out
    assert "bare (Ensembl/NCBI style)" in out
    assert "chr-prefixed (UCSC style)" in out
    # The conclusion, which is the part that says this is a naming problem
    # and not the wrong genome.
    assert "The contigs are otherwise identical" in out
    assert "Add a renaming step" in out


def test_a_single_file_prints_every_dimension_with_its_source(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(["inspect", fixture("tiny_bare.fasta")])

    out = capsys.readouterr().out
    assert code == 0, out
    assert "Reference" in out
    # Known: the value and where it was read from.
    assert "reference = 2 contigs, 40 bp (from FASTA contigs:" in out
    assert "contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines:" in out
    # Applicable but unknown: the reason, which is what a user can act on.
    assert "organism is unknown (a FASTA does not record its organism" in out
    assert "release is unknown (" in out


def test_a_dimension_that_does_not_apply_is_not_reported_as_a_gap(
    capsys: pytest.CaptureFixture[str],
) -> None:
    main(["inspect", fixture("tiny_bare.fasta")])

    out = capsys.readouterr().out
    # The category error this surface is most likely to make: a FASTA has no
    # quality encoding to be missing, and telling a user it is unknown sends
    # them looking for a declaration that does not exist.
    assert "quality_encoding is unknown" not in out
    assert "pairing is unknown" not in out
    assert "do not apply to a Reference" in out


def test_one_file_says_what_a_second_one_would_buy(
    capsys: pytest.CaptureFixture[str],
) -> None:
    main(["inspect", fixture("tiny_bare.fasta")])

    out = capsys.readouterr().out
    assert "nothing to compare" in out
    assert "two or more" in out


def test_files_that_agree_say_so_and_say_on_what(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(["inspect", fixture("tiny_bare.fasta"), fixture("tiny_bare.sam")])

    out = capsys.readouterr().out
    assert code == 0, out
    assert "no disagreement" in out
    # Which dimensions the verdict covers. "They agree" without this is a
    # claim a reader cannot size.
    assert "compared on reference and contig_naming" in out


def test_files_with_nothing_in_common_do_not_claim_to_agree(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(["inspect", fixture("reads_phred33_1.fastq"), fixture("tiny_bare.bed")])

    out = capsys.readouterr().out
    # Unaligned reads and genomic intervals share nothing but organism, which
    # neither file records. A bare "no disagreement" would be false comfort.
    assert code == 0, out
    assert "nothing was compared" in out
    assert "no dimension is stated by two" in out


def test_a_missing_file_is_a_different_answer_from_a_mismatch(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    code = main(["inspect", str(tmp_path / "nowhere.fasta")])

    captured = capsys.readouterr()
    # 2, not 1: nothing disagreed, we could not look.
    assert code == 2
    assert "no such file" in captured.err


def test_an_extension_the_loaders_do_not_know_says_which_are_known(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    odd = tmp_path / "mystery.dat"
    odd.write_text("whatever\n")

    code = main(["inspect", str(odd)])

    captured = capsys.readouterr()
    assert code == 2
    assert ".dat" in captured.err
    # Naming what *is* known turns a refusal into an answer.
    assert ".fasta" in captured.err
    assert ".tsv" in captured.err


def test_a_directory_is_told_apart_from_a_missing_file(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # `ligase inspect refs/` is an easy slip, and "no such file" about a
    # directory that is plainly there is the kind of answer that makes a
    # reader distrust everything else the tool says.
    code = main(["inspect", str(Path(fixture("tiny_bare.fasta")).parent)])

    assert code == 2
    assert "is a directory" in capsys.readouterr().err


def test_one_unreadable_file_withholds_the_verdict_on_the_others(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    broken = tmp_path / "no_id_column.tsv"
    broken.write_text("gene\tlogfc\nTP53\t2.1\n")

    code = main(
        [
            "inspect",
            fixture("tiny_bare.fasta"),
            fixture("tiny_prefixed.sam"),
            str(broken),
        ]
    )

    captured = capsys.readouterr()
    assert code == 2, captured.out
    # The two that loaded are still described: inspect says everything it can
    # see.
    assert "Reference" in captured.out
    assert "Alignment" in captured.out
    # But no verdict, because a verdict over two of three files reads as a
    # verdict over all three.
    assert "not comparing" in captured.out
    assert "ContigNamingError" not in captured.out
    assert "no_id_column.tsv" in captured.err


def test_two_gene_lists_that_each_state_nothing_are_refused(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(
        [
            "inspect",
            fixture("entities_mixed.tsv"),
            fixture("entities_header_only.tsv"),
        ]
    )

    out = capsys.readouterr().out
    # M8's failure in its purest form, reached through M9's surface: two
    # lists that say nothing about their identifier space would otherwise
    # agree by silence. Neither *states* the namespace, so there is nothing
    # for `comparable` to list — and the exit code must still be 1, because
    # failing to answer a demanded question is not agreement.
    assert code == 1, out
    assert "UndeclaredDimensionError" in out
    assert "identifier_namespace" in out


def test_every_file_given_gets_its_own_say(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # The handles are the keys the comparison is given, so two files sharing
    # a basename must not collapse into one input with the second silently
    # dropped. These two disagree about contig naming, and the only way to
    # see that is for both to be there.
    bare, prefixed = tmp_path / "a", tmp_path / "b"
    for directory, source in ((bare, "tiny_bare.sam"), (prefixed, "tiny_prefixed.sam")):
        directory.mkdir()
        (directory / "sample.sam").write_text(Path(fixture(source)).read_text())

    code = main(["inspect", str(bare / "sample.sam"), str(prefixed / "sample.sam")])

    out = capsys.readouterr().out
    assert code == 1, out
    assert str(bare / "sample.sam") in out
    assert str(prefixed / "sample.sam") in out
    assert "ContigNamingError" in out


def test_the_same_file_twice_is_still_two_files(
    capsys: pytest.CaptureFixture[str],
) -> None:
    code = main(["inspect", fixture("tiny_bare.fasta"), fixture("tiny_bare.fasta")])

    out = capsys.readouterr().out
    assert code == 0, out
    # Two blocks, not one: a repeated path is a degenerate request, but
    # answering it with half the files is a wrong answer rather than a
    # terse one.
    assert out.count("Reference ") == 2


def test_nothing_runs_and_no_cache_appears(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    monkeypatch.chdir(tmp_path)
    main(["inspect", fixture("tiny_bare.fasta")])
    capsys.readouterr()

    # `inspect` reads headers. It has no graph to build and nothing to key,
    # so it must not leave a cache behind in the directory it was run from.
    assert list(tmp_path.iterdir()) == []


def test_a_tsv_is_read_as_a_molecule_table_when_its_header_says_so(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # Two kinds share the `.tsv` suffix, and a suffix cannot tell them
    # apart, so the header does: a table with an `inchikey` column is a
    # molecule set. That is as strong a marker as a VCF's `##fileformat`
    # and not a guess -- nothing but a table of molecules carries it.
    code = main(["inspect", fixture("molecules_library.tsv")])
    assert code == 0

    said = capsys.readouterr().out
    assert "MoleculeSet" in said
    assert "InChIKey, salt-stripped and charge-neutralised" in said
    # And it is not read as a gene list, which would have inferred a
    # namespace off the keys and described the file as something it is not.
    # The namespace is reported as a category error rather than as a gap,
    # which is the distinction this surface exists to draw.
    assert "EntitySet" not in said
    assert "identifier_namespace do not apply to a MoleculeSet" in said


def test_a_tsv_with_no_inchikey_column_is_still_a_gene_list(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # The other half of the same dispatch, asserted so that adding the
    # molecule reading cannot have quietly taken the gene lists with it.
    code = main(["inspect", fixture("entities_hgnc.tsv")])
    assert code == 0

    said = capsys.readouterr().out
    assert "EntitySet" in said
    # Its space is unknown rather than guessed, because bare symbols carry
    # no marker (D56) — but it is read as a gene list, which is what this
    # half of the dispatch asserts.
    assert "identifier_namespace is unknown" in said
    assert "indistinguishable by pattern" in said


def test_two_compound_tables_standardised_differently_are_refused(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # The ten-second trial on the failure this milestone exists for: two
    # files off a shared drive, no pipeline, and a non-zero exit so the
    # answer survives a shell pipeline.
    code = main(
        [
            "inspect",
            fixture("molecules_library.tsv"),
            fixture("molecules_hits_salts.tsv"),
        ]
    )
    assert code == 1

    said = capsys.readouterr().out
    assert "representation" in said
    assert "Were the two sets standardised differently?" in said


def test_a_tsv_keyed_by_smiles_is_read_as_a_molecule_table(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # #41: the dispatch asked for an `inchikey` column and nothing else, so
    # a table of structures -- the one shape `lg.chem.standardize` is
    # specified to consume, since an InChIKey cannot be turned back into a
    # structure -- went to the gene-list loader, which refused it for having
    # no `id` column. `lg.molecule_set` loaded the same file happily, so
    # `inspect` was reporting as unreadable a file the engine reads.
    code = main(["inspect", fixture("molecules_smiles_only.tsv")])
    assert code == 0

    said = capsys.readouterr().out
    assert "MoleculeSet" in said
    # And the interesting answer, which the loader already had: the
    # structures are the names, so the state is observed rather than
    # missing.
    assert "SMILES, as given" in said
    assert "could not be read" not in said


def test_a_table_with_identifiers_is_a_gene_list_even_with_structures_in_it(
    capsys: pytest.CaptureFixture[str],
) -> None:
    # The ambiguity the wider dispatch creates, pinned: a gene list may
    # carry a structure as an attribute, and `id` is what says the names are
    # identifiers rather than structures. A molecule table has no reason to
    # have one.
    code = main(["inspect", fixture("entities_with_structures.tsv")])
    assert code == 0

    said = capsys.readouterr().out
    assert "EntitySet" in said
    assert "MoleculeSet" not in said
