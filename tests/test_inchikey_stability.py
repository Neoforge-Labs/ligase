"""M17: the measurement §4.4 asked for, kept where it can be re-run.

> Measure InChIKey stability across RDKit versions before committing to it as
> a canonical form.

It was measured first, over six releases spanning four years — 2022.09.5,
2023.09.6, 2024.09.6, 2025.09.6, 2026.03.6 and 2026.09.1 — with thirty input
structures in six standardisation states each. 1,080 keys, and **not one
disagreement** between versions. So the key is a sound thing to compare by,
and the architecture page records the method and the numbers.

What this file keeps is the part that can be checked on one interpreter: the
values the design quotes. Each is an assertion about what RDKit does, not
about what Ligase does, so if a future release changes one of them this is
where it surfaces — and that would be a finding about InChIKey rather than a
bug in Ligase.

It skips where RDKit is absent, because a genomics install is a supported
install and not a broken one. Running the full cross-version measurement
again needs several interpreters and is not a unit test; the probe that does
it is `scripts/measure_inchikey_stability.py`.
"""

from __future__ import annotations

import pytest

pytest.importorskip(
    "rdkit",
    reason="the measurement needs RDKit: pip install 'ligase-bio[chem]'",
)

from rdkit import Chem, RDLogger
from rdkit.Chem.MolStandardize import rdMolStandardize

RDLogger.DisableLog("rdApp.*")  # type: ignore[attr-defined]


def key(smiles: str, *operations: str) -> str:
    """The InChIKey of `smiles` after `operations`, in pipeline order."""
    molecule = rdMolStandardize.Cleanup(Chem.MolFromSmiles(smiles))
    for operation in operations:
        molecule = {
            "salt-stripped": rdMolStandardize.FragmentParent,
            "charge-neutralised": rdMolStandardize.Uncharger().uncharge,
            "tautomer-normalised": rdMolStandardize.TautomerEnumerator().Canonicalize,
        }[operation](molecule)
    return str(Chem.MolToInchiKey(molecule))  # type: ignore[no-untyped-call]


def test_one_substance_has_three_keys_depending_on_its_standardisation() -> None:
    # The headline of §3.3, and the clearest single reason `representation`
    # is a dimension. Aspirin, arriving as its sodium salt as a vendor
    # would ship it.
    salt = "CC(=O)Oc1ccccc1C(=O)[O-].[Na+]"
    assert key(salt) == "JZLOKWGVGHYBKD-UHFFFAOYSA-M"
    assert key(salt, "salt-stripped") == "BSYNRYMUTXBXSQ-UHFFFAOYSA-M"
    assert (
        key(salt, "salt-stripped", "charge-neutralised")
        == "BSYNRYMUTXBXSQ-UHFFFAOYSA-N"
    )
    # And the third is plain aspirin, which is the point: only the fully
    # standardised form matches a neutral library.
    assert key("CC(=O)Oc1ccccc1C(=O)O") == "BSYNRYMUTXBXSQ-UHFFFAOYSA-N"


def test_salt_stripping_alone_does_not_match_a_neutral_set() -> None:
    # The practical trap, stated as its own assertion because it is the one
    # people get wrong: `FragmentParent` leaves the deprotonated
    # carboxylate, so "salts removed" is not the same claim as "salts
    # removed and charges neutralised".
    assert key("CC(=O)Oc1ccccc1C(=O)[O-].[Na+]", "salt-stripped") != key(
        "CC(=O)Oc1ccccc1C(=O)O"
    )


def test_two_tautomers_are_two_compounds_until_they_are_normalised() -> None:
    # Warfarin's keto and enol forms. Raw they are two keys; normalised they
    # are one — so a normalised set and a raw set overlap in *nothing* for
    # this compound, and warfarin is not an exotic molecule.
    keto = "CC(=O)CC(c1ccccc1)c1c(O)c2ccccc2oc1=O"
    enol = "CC(O)=CC(c1ccccc1)c1c(O)c2ccccc2oc1=O"
    assert key(keto) != key(enol)
    assert key(keto, "tautomer-normalised") == key(enol, "tautomer-normalised")


def test_tautomer_normalisation_discards_stereochemistry() -> None:
    # §4.4 of the v0.2 design said stereo was kept. It is not, and this is
    # the assertion that says so: RDKit removes stereochemistry at an sp3
    # centre inside a tautomeric system, and a carboxylic acid's
    # alpha-carbon is inside one.
    #
    # Two enantiomers are routinely one active compound and one inactive
    # one. This is why `stereo-discarded` is an operation `representation`
    # records rather than a setting a user chooses.
    s_form = "C[C@@H](c1ccc(CC(C)C)cc1)C(=O)O"
    r_form = "C[C@H](c1ccc(CC(C)C)cc1)C(=O)O"
    assert key(s_form) == "HEFNNWSXXWATRW-JTQLQIEISA-N"
    assert key(r_form) == "HEFNNWSXXWATRW-SNVBAGLBSA-N"

    collapsed = "HEFNNWSXXWATRW-UHFFFAOYSA-N"
    assert key(s_form, "tautomer-normalised") == collapsed
    assert key(r_form, "tautomer-normalised") == collapsed


def test_a_tautomer_choice_can_move_the_skeleton_block_as_well() -> None:
    # Tetracycline. The first block of the key is the connectivity hash, so
    # a different canonical tautomer is not a near-miss: the two keys look
    # like two unrelated substances.
    tetracycline = (
        "C[C@]1(O)[C@@H]2C[C@H]3[C@@H](N(C)C)C(=O)C(C(N)=O)=C(O)[C@@]3(O)"
        "C(=O)C2=C(O)c2c(O)cccc21"
    )
    assert key(tetracycline).startswith("NWXMGUDVXFXRIG")
    assert key(tetracycline, "tautomer-normalised").startswith("MUHBBHLZPHKTTR")


def test_the_key_of_a_compound_is_not_its_canonical_smiles() -> None:
    # §3.2, which is why the identity is part of `representation` and why a
    # set named by structures is a weaker thing than one named by keys.
    # RDKit's canonical SMILES for cisplatin changed between 2024.09.6
    # (`Cl[Pt]Cl.N.N`) and 2025.09.6 (`N.N.[Cl][Pt][Cl]`) while its
    # InChIKey did not move at all.
    #
    # So this asserts the key and deliberately does **not** assert the
    # SMILES: the key is the thing that held still, and pinning the string
    # that moved would make this test fail on an upgrade that changed no
    # chemistry — which is the very cost a canonical form over SMILES would
    # have imposed on every cache.
    assert key("N.N.Cl[Pt]Cl") == "LXZZYRPGZAFOLE-UHFFFAOYSA-L"
