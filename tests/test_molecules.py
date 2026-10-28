"""M17: the molecule kinds on the Python surface, and how they were made.

The same substance has different InChIKeys depending on what was done to it
before the key was computed. Of thirty ordinary drugs measured across six
RDKit releases, twelve change key under a different standardisation state;
aspirin has three. So two compound sets prepared differently overlap in
nothing for the substances affected while the rest still match, and the hit
rate that comes out is small, non-zero and reads as poor enrichment. That is
what `representation` exists to break, and what admits the kinds under D35.

Nothing here needs RDKit. Loading a molecule table, reading its
representation and refusing to join two that disagree is the engine's work,
and the engine has no chemistry (D49). `test_chem.py` is the half that does.
"""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import KINDS, MoleculeRanking, MoleculeSet
from tests.conftest import fixture

#: The four axes a molecule table claims.
AXES = frozenset({"organism", "representation", "subject", "release"})


def molecules(name: str, **declared: object) -> MoleculeSet:
    """Load a committed molecule fixture as a set."""
    return lg.molecule_set(fixture(name), **declared)  # type: ignore[arg-type]


def ranking(name: str, **declared: object) -> MoleculeRanking:
    """Load a committed molecule fixture as a ranking."""
    return lg.molecule_ranking(fixture(name), **declared)  # type: ignore[arg-type]


# ---------------------------------------------------------- the dimensions --


def test_the_molecule_kinds_are_kinds_a_step_may_annotate() -> None:
    assert KINDS["MoleculeSet"] is MoleculeSet
    assert KINDS["MoleculeRanking"] is MoleculeRanking
    for kind in (MoleculeSet, MoleculeRanking):
        assert issubclass(kind, lg.Dataset)


def test_a_molecule_set_claims_four_axes_and_no_coordinate_system() -> None:
    # Asking a compound table for its coordinates is a category error, not
    # a gap: a list of substances has no coordinate system to have.
    loaded = molecules("molecules_library.tsv", organism="Homo sapiens")
    assert set(loaded.applicable) == set(AXES)
    for absent in ("coordinates", "sample_set", "reference", "pairing"):
        assert absent not in loaded.applicable


def test_a_molecule_set_has_no_identifier_namespace() -> None:
    # An InChIKey is an identifier space, and `representation` already says
    # which space this file is in. Two axes carrying one fact is one too
    # many.
    assert "identifier_namespace" not in molecules("molecules_library.tsv").applicable
    assert "source_set" not in molecules("molecules_library.tsv").applicable


def test_a_ranking_claims_exactly_what_the_set_claims() -> None:
    # D41's confinement rule, which is also what D48 rests on: the two
    # kinds differ in their canonical form and in nothing else.
    one = molecules("molecules_ranked.tsv", organism="Homo sapiens", subject="EGFR")
    other = ranking("molecules_ranked.tsv", organism="Homo sapiens", subject="EGFR")
    assert set(one.applicable) == set(other.applicable)
    for axis in AXES:
        assert one.dimension(axis) == other.dimension(axis), axis


# ------------------------------------------------------------- the headline --


def test_two_sets_standardised_differently_refuse_to_join() -> None:
    # The milestone's headline, and the measurement in two fixtures. Both
    # hold aspirin and metformin: one salt-stripped and neutralised, the
    # other as the vendor wrote them, as a sodium salt and a
    # hydrochloride. Their InChIKeys have nothing in common, so an overlap
    # of the two is empty and nothing about either column says why.
    with pytest.raises(lg.DimensionError) as refused:
        lg.check_step(
            "overlap",
            {
                "library": molecules("molecules_library.tsv"),
                "hits": molecules("molecules_hits_salts.tsv"),
            },
        )

    said = str(refused.value)
    assert "representation" in said
    assert "salt-stripped" in said
    assert "as given" in said
    assert "Were the two sets standardised differently?" in said
    # And where each value came from: nobody declared these, the engine
    # read them off the files.
    assert "the representation line the standardiser wrote" in said


def test_a_difference_names_the_operation_one_side_lacks() -> None:
    # The two value lines state both states in full. What they cannot do is
    # point at the operation that makes them differ.
    with pytest.raises(lg.DimensionError) as refused:
        lg.check_step(
            "overlap",
            {
                "library": molecules("molecules_library.tsv"),
                "normalised": molecules("molecules_tautomer_normalised.tsv"),
            },
        )
    assert "normalised also lists tautomer-normalised and stereo-discarded" in str(
        refused.value
    )


def test_two_sets_that_each_say_nothing_do_not_agree_by_silence() -> None:
    # `representation` is demanded wherever two molecule tables are joined,
    # which is D38's argument for demanding a namespace: a user can always
    # meet it -- declare it, or standardise -- and two sets that say
    # nothing would otherwise overlap in whatever their two toolchains
    # happened to agree on.
    with pytest.raises(lg.UndeclaredDimensionError) as refused:
        lg.check_step(
            "overlap",
            {
                "a": molecules("molecules_no_representation.tsv"),
                "b": molecules("molecules_no_representation.tsv"),
            },
        )
    assert "representation" in str(refused.value)


def test_a_ranking_and_a_set_are_the_commonest_join_and_are_checked() -> None:
    # M12's lesson: the mixed pair is the commonest join of these two kinds
    # and must not be left to a catch-all.
    with pytest.raises(lg.DimensionError):
        lg.check_step(
            "score",
            {
                "library": molecules("molecules_library.tsv"),
                "hits": ranking(
                    "molecules_ranked.tsv", representation="inchikey; as given"
                ),
            },
        )


# ----------------------------------------------------------------- loading --


def test_a_representation_is_read_off_the_line_the_standardiser_wrote() -> None:
    read = molecules("molecules_library.tsv").dimension("representation")
    assert read is not None
    assert read.value == "InChIKey, salt-stripped and charge-neutralised"
    assert read.source.startswith("from the representation line")


def test_a_table_of_keys_with_no_line_leaves_the_axis_unknown() -> None:
    # Never "as given". A ChEMBL download *is* standardised and a vendor
    # list is not, so assuming the weaker state would be the engine
    # inventing the most dangerous of the possible answers.
    loaded = molecules("molecules_no_representation.tsv")
    assert loaded.dimension("representation") is None
    assert "declare representation" in (loaded.unknown_reason("representation") or "")


def test_a_table_of_structures_says_its_names_are_structures() -> None:
    read = molecules("molecules_smiles_only.tsv").dimension("representation")
    assert read is not None
    assert read.value == "SMILES, as given"


def test_a_declared_representation_wins_and_is_recorded() -> None:
    read = molecules(
        "molecules_library.tsv", representation="inchikey; tautomer-normalised"
    ).dimension("representation")
    assert read is not None
    assert read.value == "InChIKey, tautomer-normalised"
    assert read.source == "declared"


def test_a_representation_the_engine_cannot_read_is_refused_at_the_call() -> None:
    # Checked in the binding rather than only in the loader, so that a
    # declaration the engine would silently ignore is a `ValueError` naming
    # what it does know.
    with pytest.raises(ValueError, match="desalted-twice"):
        molecules("molecules_library.tsv", representation="inchikey; desalted-twice")
    with pytest.raises(ValueError, match="inchikey"):
        molecules("molecules_library.tsv", representation="fingerprint")


def test_a_table_with_no_identity_column_is_not_a_molecule_set() -> None:
    with pytest.raises(lg.LoadError) as refused:
        molecules("molecules_no_identity.tsv")
    said = str(refused.value)
    assert "inchikey" in said
    assert "smiles" in said
    assert "activity_nm" in said, "it must say what the file does have"


def test_an_operation_the_engine_does_not_know_is_not_a_representation() -> None:
    with pytest.raises(lg.LoadError) as refused:
        molecules("molecules_unknown_operation.tsv")
    said = str(refused.value)
    assert "desalted-twice" in said
    assert "tautomer-normalised" in said, "it must name the four it knows"


def test_a_molecule_table_does_not_record_its_organism_subject_or_release() -> None:
    loaded = molecules("molecules_library.tsv")
    for axis in ("organism", "subject", "release"):
        assert loaded.dimension(axis) is None, axis
        assert loaded.unknown_reason(axis), f"{axis} must say why it is unknown"
