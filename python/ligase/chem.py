"""Standardising molecules, and the one place in Ligase that needs RDKit.

    from ligase.types import MoleculeSet

    clean = lg.chem.standardize(
        library, to="inchikey; salt-stripped, charge-neutralised"
    )

Everything else about molecules — the two kinds, the `representation`
dimension, the refusal to join two sets that disagree — is in the engine and
needs no chemistry at all. Only this module does, and it imports RDKit
**inside** the function that uses it, so that `import ligase` on a genomics
install imports nothing from a toolkit that is not there.

## Why the engine has no chemistry (D49)

Computing an InChIKey needs RDKit. RDKit is a Python package, so an extra can
reach it — unlike the remote store's cargo feature, which is why *that* extra
is empty. Putting it in the Rust core instead would mean linking an InChI
library, and the promise `tests/default_build.rs` defends is that
`pip install ligase-bio` needs no toolchain beyond Rust.

So the division is: the engine refuses to connect two sets whose
standardisation disagrees, which needs no chemistry; this module produces a
set whose standardisation is known, which needs all of it.

    pip install 'ligase-bio[chem]'

## Why standardisation is a conversion

`convert`'s rule is that a conversion crosses one axis and can **fail** for
some inputs — "a conversion that could not fail would be a rename". This one
crosses `representation`, reads its destination from the same `to=` every
conversion reads, and fails for a structure that does not parse or a mixture
with no single parent. Those failures are named and counted rather than
dropped, which is the contract `map_identifiers` already keeps (D26).

It is the registry's second entry and the first whose body is Python. Nothing
in the registry had to change for that.

## What the measurement found, and what it costs you

Six RDKit releases over four years produced **identical** InChIKeys for all
thirty molecules in every standardisation state — so the key is a sound thing
to compare by. In the same span RDKit's canonical SMILES for cisplatin
changed, which is why a set named by structures is a weaker thing than one
named by keys and why this step writes keys.

What moves the key is the standardisation, and there is one consequence worth
knowing before you ask for it: **RDKit's tautomer canonicaliser discards
stereochemistry.** Ibuprofen's two enantiomers both come out as
`HEFNNWSXXWATRW-UHFFFAOYSA-N`. §4.4 of the v0.2 design said stereo was kept;
it is not. So `tautomer-normalised` usually has to be asked for together with
`stereo-discarded`, and this step checks that asymmetrically — see
`standardize`.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from .errors import StepDefinitionError
from .graph import step
from .recipes import write
from .types import MoleculeRanking, MoleculeSet

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from .recipes import Noticed

__all__ = ["standardize", "standardize_ranking", "toolkit_version"]

#: The extra to install, named in every error this module raises for want of
#: it. One spelling, in one place, so the instruction cannot drift.
EXTRA = "pip install 'ligase-bio[chem]'"

#: How many molecules that did not standardise are named individually.
#:
#: The rest are carried by the count, for the reason `map_identifiers` caps
#: its own at twenty: a library of two hundred thousand compounds run through
#: a standardiser that rejects a fraction of them would otherwise put
#: thousands of rows in provenance and interpolate all of them into one
#: methods sentence. Twenty is enough to recognise a pattern.
NAMED_AT_MOST = 20

#: The line a standardised table carries, and the loader reads.
REPRESENTATION_LINE = "#ligase representation:"


def toolkit_version() -> str:
    """The version of RDKit this interpreter has.

    Called before a `standardize` step is built rather than inside it, so
    that the version is one of the step's **parameters** and joins its cache
    key. Read inside the body it would not, and the step would then serve a
    cached answer across an upgrade that could have changed every key it
    computed — which is exactly the failure D42's version-capture clause was
    written for, where upgrading `bwa` moved no key at all.

    Returns:
        The version, as RDKit reports it, e.g. `2026.03.6`.

    Raises:
        StepDefinitionError: If RDKit is not installed, naming the extra.

    Note:
        The version is a step **parameter** rather than a `tool=`, and that
        is a limit rather than a preference. `tool=` means a program the
        engine runs to ask its version, and it is also what a citation
        resolves from. A library is neither, so RDKit's version reaches the
        cache key -- the half that matters for correctness -- and RDKit is
        not credited in a methods section, which is the half that matters
        for a reference list. See the architecture page.
    """
    try:
        import rdkit
    except ImportError as missing:  # pragma: no cover - depends on the install
        raise StepDefinitionError(
            "standardising molecules needs RDKit, which this install does not "
            f"have. {EXTRA}. Everything else about molecule sets -- loading "
            "them, comparing their representations, refusing to join two that "
            "disagree -- needs no chemistry and works on a default install."
        ) from missing
    return str(rdkit.__version__)


def standardize(molecules: MoleculeSet, *, to: str) -> MoleculeSet:
    """Standardise a set of molecules and name them by InChIKey.

    The output **declares** the state it has arrived in, by carrying a
    representation line, so everything downstream is checked against a true
    statement rather than against a claim nobody verified.

    Args:
        molecules: The set to standardise. It needs a `smiles` column:
            standardisation works on structures, and an InChIKey cannot be
            turned back into one.
        to: The state to arrive in, spelled as a representation —
            `inchikey; salt-stripped, charge-neutralised`. The identity must
            be `inchikey`, because that is what this step computes and what
            four years of RDKit releases did not move.

    Returns:
        A recipe, typed as the molecule set the step produces.

    Raises:
        StepDefinitionError: If RDKit is not installed, or if `to` is not a
            representation this step can arrive at.

    Note:
        **Stereochemistry and tautomers interact, and the check is
        asymmetric.** Asking for `tautomer-normalised` usually discards
        stereochemistry, because RDKit removes it at an sp3 centre inside a
        tautomeric system and a carboxylic acid's alpha-carbon is inside
        one. Declaring `stereo-discarded` when nothing was discarded is
        *safe* — it can only refuse a join you fix by declaring one thing
        consistently — so it is a note. Not declaring it when stereo *was*
        discarded is *dangerous*, because two sets would then agree while
        one of them has lost the difference between an active enantiomer and
        an inactive one, so it is an error naming how many molecules lost
        it.

        Molecules that do not standardise — a structure that does not parse,
        a mixture with no single parent — are left out and named, never
        dropped in silence, so the output being smaller than the input is a
        recorded fact (D26).
    """
    return _standardize(molecules, to=_checked(to), rdkit_version=toolkit_version())


def standardize_ranking(molecules: MoleculeRanking, *, to: str) -> MoleculeRanking:
    """Standardise a ranking of molecules, keeping its order.

    Everything `standardize` does, for the kind whose row order is the
    answer. The order is kept exactly: a molecule that does not standardise
    is left out, and every row below it moves up, so a cut-off taken after
    this step is not the cut-off taken before it.

    Args:
        molecules: The ranking to standardise, in rank order.
        to: As `standardize`.

    Returns:
        A recipe, typed as the molecule ranking the step produces.

    Raises:
        StepDefinitionError: As `standardize`.
    """
    return _standardize_ranking(
        molecules, to=_checked(to), rdkit_version=toolkit_version()
    )


def _set_body(molecules: MoleculeSet, *, to: str, rdkit_version: str) -> MoleculeSet:
    """Standardise a set, with the toolkit version in the cache key."""

    def produce(out: Path) -> list[Noticed]:
        return _run(molecules.file, out, to)

    return write(produce)


def _ranking_body(
    molecules: MoleculeRanking, *, to: str, rdkit_version: str
) -> MoleculeRanking:
    """Standardise a ranking, with the toolkit version in the cache key."""

    def produce(out: Path) -> list[Noticed]:
        return _run(molecules.file, out, to)

    return write(produce)


# A step's name is what provenance, the run report, the methods paragraph and
# every error message call it -- and a user wrote `lg.chem.standardize`, not
# the private function that carries the toolkit version into the cache key.
# `@lg.step` takes the name off the function it decorates, so the name is set
# here rather than leaving `_set_body` to surface in a methods section
# somebody is going to publish.
_set_body.__name__ = "standardize"
_ranking_body.__name__ = "standardize_ranking"

_standardize = step(tool="ligase", conversion="standardize")(_set_body)
_standardize_ranking = step(tool="ligase", conversion="standardize")(_ranking_body)


def _run(source: str, out: Path, to: str) -> list[Noticed]:
    """Standardise `source` into `out`, and say what that cost.

    One body for both kinds. The rows are written in the order they were
    read, which is what a ranking needs and what a set does not care about —
    so the difference between the two kinds stays in their canonical forms,
    where D41 put it, rather than leaking in here as a flag.

    Raises:
        ValueError: If the table has no `smiles` column to standardise, or
            if stereochemistry was discarded without being declared.
    """
    from rdkit import Chem, RDLogger
    from rdkit.Chem.MolStandardize import rdMolStandardize

    # RDKit writes parse failures to its own global logger. They are this
    # function's business to report as notes, not its business to print.
    RDLogger.DisableLog("rdApp.*")  # type: ignore[attr-defined]

    wanted = _target(to)
    comments, header, rows = _read(source)
    at = header.index("smiles")

    # Applied in this order whatever order `to` listed them in, because the
    # order is chemistry rather than presentation: salts come off first, then
    # charges, then a tautomer is chosen. It is the order the measurement in
    # `scripts/measure_inchikey_stability.py` used, so the keys this writes
    # are the keys that were measured. The representation line lists a *set*,
    # which is why its order can differ from this one without meaning
    # anything.
    operations = [
        operation
        for name, operation in (
            ("salt-stripped", rdMolStandardize.FragmentParent),
            ("charge-neutralised", rdMolStandardize.Uncharger().uncharge),
            ("tautomer-normalised", rdMolStandardize.TautomerEnumerator().Canonicalize),
        )
        if name in wanted
    ]

    out_header = header if "inchikey" in header else ["inchikey", *header]
    key_at = out_header.index("inchikey")

    written: list[list[str]] = []
    failed: list[str] = []
    seen: set[str] = set()
    collided: set[str] = set()
    lost_stereo = 0
    for row in rows:
        # A row the header cannot account for is refused by the loader, so a
        # file a user named cannot get here -- but an intermediate written by
        # another step is not re-loaded before this runs. An empty structure
        # is a structure nothing can parse, which is reported as a failure
        # below; an IndexError would be a traceback about somebody else's
        # file.
        structure = row[at] if at < len(row) else ""
        molecule = Chem.MolFromSmiles(structure)
        if molecule is not None:
            molecule = rdMolStandardize.Cleanup(molecule)
        if molecule is None:
            failed.append(structure)
            continue
        had_stereo = _has_stereo(molecule)
        for operation in operations:
            molecule = operation(molecule)
            if molecule is None:
                break
        if molecule is None:
            failed.append(structure)
            continue
        if had_stereo and not _has_stereo(molecule):
            lost_stereo += 1
        key = Chem.MolToInchiKey(molecule)  # type: ignore[no-untyped-call]
        if not key:
            failed.append(structure)
            continue
        fields = list(row) if "inchikey" in header else ["", *row]
        fields[key_at] = key
        written.append(fields)
        if key in seen:
            collided.add(key)
        seen.add(key)

    _check_stereo(lost_stereo, wanted, len(written))
    _write(out, to, comments, out_header, written)

    noticed: list[Noticed] = [
        ("molecule-did-not-standardise", structure)
        for structure in failed[:NAMED_AT_MOST]
    ]
    if failed:
        noticed.append(("molecules-did-not-standardise", f"count={len(failed)}"))
    # Standardising is what brings two rows together -- a salt and its
    # parent are two structures and one substance -- so both rows are kept
    # and the collision is named. Dropping one would make the output quietly
    # smaller and choosing one would be a guess about whose attributes to
    # carry, which is D38's argument against coercing identifiers applied
    # unchanged.
    noticed += [
        ("molecule-not-unique", key) for key in sorted(collided)[:NAMED_AT_MOST]
    ]
    if "stereo-discarded" in wanted and lost_stereo == 0:
        noticed.append(("stereo-not-discarded", ""))
    return noticed


def _checked(to: str) -> str:
    """`to` in the engine's own spelling, refused now rather than at run time.

    Checked here, while the pipeline file is still being read, so that a
    state nobody can arrive at is a failure before any step runs — which is
    where this engine puts every other failure it can see early.

    The engine does the parsing. One validator decides what a representation
    is, so an unknown operation is refused in the same words whether it was
    written in a file, in a declaration or here; and the spelling that comes
    back is canonical, so the line the step writes is the line its own loader
    reads back to the same value.
    """
    from . import _core

    identity = to.partition(";")[0].strip().lower()
    if identity != "inchikey":
        raise StepDefinitionError(
            f"`standardize` arrives at an InChIKey, so `to` must name "
            f"`inchikey`, not `{to.partition(';')[0].strip()}`. A set named by "
            "structures has an identity its own toolkit can move under it: "
            "RDKit's canonical SMILES for cisplatin changed between 2024.09.6 "
            "and 2025.09.6 while its InChIKey did not."
        )
    try:
        return str(_core.parse_representation(to))
    except ValueError as wrong:
        raise StepDefinitionError(str(wrong)) from wrong


def _target(to: str) -> list[str]:
    """The operations a checked `to` asks for, in the engine's order."""
    operations = to.partition(";")[2]
    return [
        named.strip()
        for named in operations.split(",")
        if named.strip() and named.strip() != "as given"
    ]


def _has_stereo(molecule: Any) -> bool:
    """Whether any atom or bond of this molecule has defined stereochemistry.

    Asked before and after, because the answer is what `stereo-discarded`
    records. A molecule that never had stereochemistry cannot have lost any,
    so a flat library does not trip the check.
    """
    from rdkit import Chem

    centres = Chem.FindMolChiralCenters(  # type: ignore[no-untyped-call]
        molecule, useLegacyImplementation=False
    )
    return bool(centres) or any(
        bond.GetStereo() != Chem.BondStereo.STEREONONE for bond in molecule.GetBonds()
    )


def _check_stereo(lost: int, wanted: list[str], total: int) -> None:
    """Refuse the dangerous half of the asymmetry, and let the safe half be.

    Raises:
        ValueError: If stereochemistry was discarded and the state asked for
            does not say so. A run-time failure rather than a definition
            one, because whether any molecule loses stereo is a fact about
            the molecules and cannot be known until they are read.
    """
    if lost and "stereo-discarded" not in wanted:
        raise ValueError(
            f"standardising discarded stereochemistry on {lost} of {total} "
            "molecules, and the state you asked for does not say so. RDKit "
            "removes stereochemistry at an sp3 centre inside a tautomeric "
            "system, and a carboxylic acid's alpha-carbon is inside one -- so "
            "ibuprofen's two enantiomers both become "
            "HEFNNWSXXWATRW-UHFFFAOYSA-N. Add `stereo-discarded` to `to=`, so "
            "this set refuses to join one that still knows which enantiomer it "
            "holds, or drop `tautomer-normalised` to keep the difference."
        )


def _read(source: str) -> tuple[list[str], list[str], list[list[str]]]:
    """A molecule table's comment lines, its header, and its rows.

    Read here rather than through the engine because the engine hands out
    dimensions, not rows. A table whose rows do not fit its header has
    already been refused by the loader, which is what typed this step's
    input, so this does not check it a second time.
    """
    comments: list[str] = []
    header: list[str] = []
    rows: list[list[str]] = []
    with open(source, encoding="utf-8") as table:
        for line in _lines(table):
            if not header:
                if line.startswith("#"):
                    comments.append(line)
                    continue
                if not line:
                    continue
                header = [name.strip() for name in line.split("\t")]
                if "smiles" not in header:
                    raise ValueError(
                        "standardising needs structures, and this table has no "
                        f"`smiles` column: its columns are {', '.join(header)}. "
                        "An InChIKey cannot be turned back into a structure, so "
                        "a table of keys alone cannot be standardised."
                    )
                continue
            if line:
                rows.append(line.split("\t"))
    return comments, header, rows


def _lines(table: Iterator[str]) -> Iterator[str]:
    r"""Each line without its ending, splitting on a lone `\r` as well.

    The engine's own reader does this and says why: Excel for Mac wrote CR
    alone, and a reader that only breaks on `\\n` reads such a file as one
    very long header and no rows -- which loads successfully and silently
    loses every molecule.
    """
    for line in table:
        parts = line.replace("\r\n", "\n").replace("\r", "\n").split("\n")
        # A line's own ending leaves one empty part behind. Dropping it here
        # rather than in the caller is what keeps a blank line in the file
        # distinguishable from the end of a line that had content.
        if parts and parts[-1] == "":
            parts.pop()
        yield from parts


def _write(
    out: Path,
    representation: str,
    comments: list[str],
    header: list[str],
    rows: list[list[str]],
) -> None:
    """Write the standardised table, its own representation line first.

    The line is the engine's canonical spelling of the state asked for, and
    `_check_stereo` has already refused the one case where what was done
    differs from what was asked. So the line is a true statement about the
    file rather than a copy of an intention — which matters, because the line
    is a dimension and the loader will read it as one.

    The input's other comment lines are carried over. They are somebody's own
    words about their own molecules, and a step that dropped them would be
    losing data to tidiness.
    """
    kept = [line for line in comments if not line.startswith(REPRESENTATION_LINE)]
    with open(out, "w", encoding="utf-8") as table:
        table.write(f"{REPRESENTATION_LINE} {representation}\n")
        for line in kept:
            table.write(f"{line}\n")
        table.write("\t".join(header) + "\n")
        for row in rows:
            table.write("\t".join(row) + "\n")
