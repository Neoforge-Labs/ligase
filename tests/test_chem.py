"""M17: the one part of Ligase that needs RDKit.

Everything in `test_molecules.py` runs on a default install, because the
engine has no chemistry (D49). This file is the other half: the step that
*produces* a set whose standardisation is known, and the three things it has
to get right — failures named rather than dropped, the toolkit's version in
the cache key, and the asymmetric stereo check.

Every test here skips where RDKit is absent. It skips rather than failing
because a genomics install is a supported install, not a broken one.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from tests.conftest import fixture

rdkit = pytest.importorskip(
    "rdkit",
    reason="standardising molecules needs RDKit: pip install 'ligase-bio[chem]'",
)

#: A vendor table: structures, salts, and the two enantiomers of ibuprofen.
#:
#: Written here rather than committed as a fixture because what it is *for*
#: is the structures, and a reader of this file should be able to see which
#: molecule each assertion is about.
VENDOR = "\n".join(
    [
        "smiles\tname\tlot",
        "CC(=O)Oc1ccccc1C(=O)[O-].[Na+]\taspirin sodium\tL1",
        "CN(C)C(=N)NC(N)=N.Cl\tmetformin hydrochloride\tL2",
        "CC(=O)CC(c1ccccc1)c1c(O)c2ccccc2oc1=O\twarfarin\tL3",
        "not a molecule at all\tmystery\tL4",
        "",
    ]
)

#: The same, flat: nothing in it has stereochemistry to lose.
FLAT = "\n".join(
    [
        "smiles\tname",
        "CC(=O)Oc1ccccc1C(=O)O\taspirin",
        "CN(C)C(=N)NC(N)=N\tmetformin",
        "",
    ]
)

#: One molecule with a stereocentre alpha to a carboxylic acid, which is
#: inside a tautomeric system and therefore loses its stereochemistry to
#: tautomer canonicalisation.
CHIRAL = "\n".join(
    [
        "smiles\tname",
        "C[C@@H](c1ccc(CC(C)C)cc1)C(=O)O\tibuprofen (S)",
        "",
    ]
)


@pytest.fixture
def table(tmp_path: Path) -> Path:
    """A writable directory for a table and a cache."""
    return tmp_path


def written(where: Path, text: str, name: str = "library.tsv") -> str:
    """A molecule table on disk."""
    path = where / name
    path.write_text(text, encoding="utf-8")
    return str(path)


def run(
    where: Path, text: str, to: str, *, ranked: bool = False, source: str | None = None
) -> lg.RunReport:
    """Standardise a table through a real pipeline.

    Through a pipeline rather than by calling the body, so that what is
    under test is the step as a user gets it: typed, checked, cached, and
    with its notes carried out to the run report the way every other note
    in this engine is.
    """
    path = source or written(where, text)
    if ranked:
        return lg.Pipeline(
            lg.chem.standardize_ranking(lg.molecule_ranking(path), to=to)
        ).run(cache=where / "cache")
    return lg.Pipeline(lg.chem.standardize(lg.molecule_set(path), to=to)).run(
        cache=where / "cache"
    )


def lines(report: lg.RunReport) -> list[str]:
    """What the standardiser wrote."""
    return Path(report.steps[-1].output).read_text(encoding="utf-8").splitlines()


def standardised(where: Path, text: str, to: str) -> list[str]:
    """Run the standardiser over `text`, and give back the output's lines."""
    return lines(run(where, text, to))


# ------------------------------------------------------ what it produces --


def test_standardising_writes_keys_and_says_what_produced_them(table: Path) -> None:
    lines = standardised(table, FLAT, to="inchikey; salt-stripped, charge-neutralised")
    assert lines[0] == (
        "#ligase representation: inchikey; salt-stripped, charge-neutralised"
    )
    assert lines[1].split("\t")[0] == "inchikey"
    # Aspirin, salt-stripped and neutralised. The measurement's own value.
    assert "BSYNRYMUTXBXSQ-UHFFFAOYSA-N" in lines[2]


def test_its_output_loads_with_the_representation_it_wrote(table: Path) -> None:
    # The round trip the design rests on: a step writes a line, and the
    # loader reads it back to the same value. Were these to drift, a
    # pipeline would standardise a set and then be unable to say so.
    report = run(table, FLAT, to="inchikey; salt-stripped, charge-neutralised")
    read = lg.molecule_set(report.steps[-1].output).dimension("representation")
    assert read is not None
    assert read.value == "InChIKey, salt-stripped and charge-neutralised"


def test_salt_stripping_alone_is_not_enough_to_match_a_neutral_set(
    table: Path,
) -> None:
    # The practical trap, measured: `FragmentParent` leaves the
    # deprotonated carboxylate, so a salt-stripped aspirin is `...-M` where
    # a neutralised one is `...-N`. Two sets that both say "salts removed"
    # still fail to overlap if only one of them was also neutralised.
    stripped = standardised(table, VENDOR, to="inchikey; salt-stripped")
    both = standardised(table, VENDOR, to="inchikey; salt-stripped, charge-neutralised")
    assert any("BSYNRYMUTXBXSQ-UHFFFAOYSA-M" in line for line in stripped)
    assert any("BSYNRYMUTXBXSQ-UHFFFAOYSA-N" in line for line in both)


def test_the_input_s_own_comments_are_carried_over(table: Path) -> None:
    # Somebody's own words about their own molecules. A step that dropped
    # them would be losing data to tidiness.
    lines = standardised(
        table, f"# lot numbers are ours, not the vendor's\n{FLAT}", to="inchikey"
    )
    assert "# lot numbers are ours, not the vendor's" in lines


# ---------------------------------------------- failures, named not dropped --


def test_a_molecule_that_does_not_standardise_is_named_and_counted(
    table: Path,
) -> None:
    # D26, by the contract `map_identifiers` already keeps. Dropped here,
    # these would be compounds missing from the rest of the analysis with
    # nothing to show why.
    report = run(table, VENDOR, to="inchikey; salt-stripped, charge-neutralised")
    notes = [note for step in report.steps for note in step.notes]

    assert any("not a molecule at all" in note for note in notes), notes
    assert any("1 molecules could not be standardised" in note for note in notes), notes
    # And the output is smaller than the input by exactly that one.
    rows = [row for row in lines(report)[2:] if row]
    assert len(rows) == 3


# ------------------------------------------- the asymmetric stereo check --


def test_discarding_stereo_without_declaring_it_is_an_error(table: Path) -> None:
    # The dangerous direction. Two sets would agree while one of them has
    # lost the difference between an active enantiomer and an inactive one,
    # which is the false agreement the engine exists to refuse.
    with pytest.raises(lg.RunError) as refused:
        run(table, CHIRAL, to="inchikey; tautomer-normalised")

    said = refused.value.report.failures[0].message
    assert "discarded stereochemistry on 1 of 1" in said
    assert "stereo-discarded" in said
    assert "HEFNNWSXXWATRW-UHFFFAOYSA-N" in said, "it must name what two become one"


def test_declaring_it_when_nothing_was_discarded_is_only_a_note(
    table: Path,
) -> None:
    # The safe direction. Claiming stereo was discarded when none was can
    # only refuse a join a user fixes by declaring one thing consistently,
    # so it is a note. The asymmetry is the one M10a settled for case
    # folding, on a second axis.
    report = run(table, FLAT, to="inchikey; stereo-discarded")
    notes = [note for step in report.steps for note in step.notes]
    assert any("no molecule in this set lost any" in note for note in notes), notes


def test_declaring_it_when_it_happened_is_neither(table: Path) -> None:
    report = run(table, CHIRAL, to="inchikey; tautomer-normalised, stereo-discarded")
    assert [note for step in report.steps for note in step.notes] == []
    assert any("HEFNNWSXXWATRW-UHFFFAOYSA-N" in line for line in lines(report))


# ------------------------------------------------- what the step refuses --


def test_the_step_arrives_at_a_key_and_says_so(table: Path) -> None:
    # A set named by structures has an identity its own toolkit can move
    # under it, measured: RDKit's canonical SMILES for cisplatin changed
    # between 2024.09.6 and 2025.09.6 while its InChIKey did not.
    with pytest.raises(lg.StepDefinitionError, match="cisplatin"):
        lg.chem.standardize(
            lg.molecule_set(written(table, FLAT)), to="smiles; salt-stripped"
        )


def test_a_state_nobody_can_arrive_at_fails_before_any_step_runs(
    table: Path,
) -> None:
    # Checked while the pipeline file is still being read, which is where
    # this engine puts every failure it can see early.
    with pytest.raises(lg.StepDefinitionError, match="desalted-twice"):
        lg.chem.standardize(
            lg.molecule_set(written(table, FLAT)), to="inchikey; desalted-twice"
        )


def test_a_table_of_keys_alone_cannot_be_standardised(table: Path) -> None:
    # An InChIKey cannot be turned back into a structure, so there is
    # nothing for the standardiser to work on.
    with pytest.raises(lg.RunError) as refused:
        run(
            table,
            "",
            to="inchikey; salt-stripped",
            source=fixture("molecules_no_representation.tsv"),
        )
    assert "smiles" in refused.value.report.failures[0].message


# --------------------------------------------- the version in the key --


def test_the_toolkit_version_is_what_this_interpreter_has() -> None:
    assert lg.chem.toolkit_version() == rdkit.__version__


def test_a_ranking_is_standardised_in_its_own_order(table: Path) -> None:
    # The rows come out in the order they went in, which is what a ranking
    # needs. The difference between the two kinds stays in their canonical
    # forms, where D41 put it.
    report = run(table, FLAT, to="inchikey; salt-stripped", ranked=True)
    rows = [row for row in lines(report)[2:] if row]
    assert [row.split("\t")[-1] for row in rows] == ["aspirin", "metformin"]


def test_the_two_steps_are_typed_as_the_kinds_they_produce() -> None:
    assert lg.chem.standardize.__annotations__["return"] == "MoleculeSet"
    assert lg.chem.standardize_ranking.__annotations__["return"] == "MoleculeRanking"


def test_a_standardised_set_declares_the_state_it_arrived_in(table: Path) -> None:
    # What makes this a conversion rather than a step that happens to write
    # molecules: the output's own representation is the state asked for, so
    # everything downstream is checked against a true statement.
    clean = lg.chem.standardize(
        lg.molecule_set(written(table, VENDOR)),
        to="inchikey; salt-stripped, charge-neutralised",
    )
    lg.Pipeline(clean).check()

    dataset = lg.handle(clean).dataset
    assert dataset is not None
    read = dataset.dimension("representation")
    assert read is not None
    assert read.value == "InChIKey, salt-stripped and charge-neutralised"
    # Named for the function the user called, not for the private one that
    # carries the toolkit version into the cache key. This is what a methods
    # paragraph prints.
    assert read.source == "from step `standardize`"


def test_a_conversion_carries_every_other_axis(table: Path) -> None:
    clean = lg.chem.standardize(
        lg.molecule_set(
            written(table, VENDOR),
            organism="Homo sapiens",
            subject="EGFR",
            release="ChEMBL 34",
        ),
        to="inchikey; salt-stripped",
    )
    lg.Pipeline(clean).check()

    dataset = lg.handle(clean).dataset
    assert dataset is not None
    for axis, expected in [
        ("organism", "Homo sapiens"),
        ("subject", "EGFR"),
        ("release", "ChEMBL 34"),
    ]:
        found = dataset.dimension(axis)
        assert found is not None, axis
        assert found.value == expected, axis


def test_the_toolkit_is_a_tool_the_engine_names_and_credits(table: Path) -> None:
    # D46. The step declares `tool="rdkit"` like any other tool, so the
    # version joins the cache key by the ordinary mechanism and the
    # citation resolves from the engine's curated table -- where before,
    # `tool=` meant a program the engine execs, so a library could be
    # neither.
    report = run(table, FLAT, to="inchikey; salt-stripped")
    assert report.steps[-1].step == "standardize", (
        "a step is named for what the user called, not for the private "
        "function that registers the version"
    )

    methods = lg.Pipeline(
        lg.chem.standardize(
            lg.molecule_set(written(table, FLAT)), to="inchikey; salt-stripped"
        )
    ).methods(cache=table / "cache")

    assert ("rdkit", f"rdkit {rdkit.__version__}") in methods.tools
    assert any("RDKit" in citation for citation in methods.citations), methods.citations
    assert methods.uncited == [], "a credited tool is not also an uncited one"
    assert f"rdkit {rdkit.__version__}" in methods.text


def test_the_version_the_engine_records_is_this_interpreters(table: Path) -> None:
    # The engine cannot read it: a library lives in this interpreter rather
    # than on PATH. So `toolkit_version` registers it, and the engine asks
    # for it by the tool's name like any other.
    assert lg.chem.toolkit_version() == rdkit.__version__
    assert lg._core.tool_version("rdkit") == f"rdkit {rdkit.__version__}"


def test_registering_a_different_version_is_refused() -> None:
    # The version is in every key beneath the step, so two answers in one
    # process would mean two keys for one computation. Reading
    # `rdkit.__version__` twice cannot disagree, so this cannot happen by
    # accident -- which is why it is refused rather than resolved.
    lg._core.register_library_version("rdkit", rdkit.__version__)
    with pytest.raises(ValueError, match="two answers in one process"):
        lg._core.register_library_version("rdkit", "0.0.0-not-really")
