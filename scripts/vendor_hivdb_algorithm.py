#!/usr/bin/env python3
"""Derive a Ligase ruleset from a vendored Stanford HIVdb ASI algorithm.

The Stanford HIVdb interpretation algorithm is published as ASI2 XML in
Stanford's own repository, `hivdb/hivfacts`, under the Unlicense -- a
public-domain dedication. `crates/ligase-core/rulesets/` holds those files
byte-for-byte, pinned by SHA-256 in `PROVENANCE.tsv`, because `CLAUDE.md`
requires remote data to be pinned by accession, version and checksum, and
because a run that reaches the network to learn what a mutation means is
not reproducible offline.

This script turns one of those XML files into a `.ruleset` -- a small
tab-separated format the engine owns and parses strictly. Both files are
committed, so the vendored XML stays auditable against upstream while the
core needs no XML parser and no new dependency.

    scripts/vendor_hivdb_algorithm.py --check          # re-derive, compare
    scripts/vendor_hivdb_algorithm.py --write          # re-derive, write

`--check` is what `tests/test_rulesets.py` runs: it re-derives every pinned
algorithm and fails if the committed `.ruleset` has drifted from the
committed XML. A derived file that nothing re-derives is a file that can
quietly stop matching its source.

## What is kept, and what is not

Kept: the algorithm's name, version and date; the level definitions; the
global score range; genes, drug classes and drugs; and the scoring rule for
every drug.

Not kept: `MUTATION_COMMENTS` and `RESULT_COMMENTS`. Those are explanatory
text attached to a mutation or a result, not part of how a level is
reached, and Ligase reports levels and scores. They are the obvious thing
to add if a report ever wants prose.

## Why the grammar can be this strict

Every HIVDB algorithm from 9.8 to 10.2 uses one shape and one only:

    SCORE FROM ( item, item, ... )

where an item is `<residue> => <score>`, several residues joined by `AND`,
or `MAX(` several such `)`. There is no `ATLEAST`, `NOTMORETHAN`, `SELECT`
or `EXCLUDE`, every drug uses `USE_GLOBALRANGE`, and the global range is
the same in both versions. Anything outside that is refused rather than
skipped, for the reason D21 gives: a parser that reads what it can and
ignores the rest reduces a file to the part it understood, and two files
differing only outside that part would agree.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path
from typing import NamedTuple

#: Where the vendored algorithms and their pins live.
RULESETS = Path(__file__).resolve().parents[1] / "crates" / "ligase-core" / "rulesets"

#: The pin file: one row per vendored algorithm.
PROVENANCE = RULESETS / "PROVENANCE.tsv"

#: The format version of a derived `.ruleset`. It goes in the first line so
#: the Rust reader can refuse a file it does not understand rather than
#: misreading one.
FORMAT = 1

#: A residue specification: a position followed by the amino acids that
#: satisfy it. `d` is a deletion and `i` an insertion, lowercase, as ASI
#: writes them.
RESIDUE = re.compile(r"\A(\d+)([A-Zdi]+)\Z")


class Item(NamedTuple):
    """One alternative within a scoring group.

    Attributes:
        residues: Every residue that must be present for the item to
            score. More than one means the rule's `AND`.
        score: What the item contributes when it matches. Negative scores
            are real -- `184V => -10` for zidovudine -- and are never
            clamped, because Stanford's own reports carry them.
    """

    residues: tuple[str, ...]
    score: int


def _text(element: ElementTree.Element, tag: str) -> str:
    """Return a child's text, refusing an absent or empty one."""
    child = element.find(tag)
    if child is None or child.text is None or not child.text.strip():
        raise ValueError(f"<{element.tag}> has no <{tag}>")
    return child.text.strip()


def _residue(token: str) -> str:
    """Canonicalise one residue specification, refusing anything else."""
    matched = RESIDUE.match(token.strip())
    if matched is None:
        raise ValueError(f"not a residue specification: {token.strip()!r}")
    position, acids = matched.groups()
    # Sorted and de-duplicated, so `215FY` and `215YF` are one thing. The
    # set is what the rule means; the order it was typed in is not.
    return f"{int(position)}{''.join(sorted(set(acids)))}"


def _split(text: str, separator: str) -> list[str]:
    """Split on a separator that is not inside parentheses."""
    parts, depth, current = [], 0, ""
    for character in text:
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
        if character == separator and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += character
    parts.append(current)
    return parts


def _item(text: str) -> Item:
    """Read one `<residues> => <score>` alternative."""
    left, _, right = text.partition("=>")
    if not right:
        raise ValueError(f"no `=>` in rule item: {text.strip()!r}")
    residues = tuple(_residue(token) for token in left.split(" AND "))
    try:
        score = int(right.strip())
    except ValueError as error:
        raise ValueError(f"not a score: {right.strip()!r}") from error
    return Item(residues, score)


def _groups(condition: str) -> list[list[Item]]:
    """Read a drug's `SCORE FROM (...)` into groups of alternatives.

    A bare item is a group of one; a `MAX(...)` is a group of several. The
    drug's score is the sum over groups of the largest matching item in
    each, which is exactly what `SCORE FROM` with `MAX` means.
    """
    body = condition.strip()
    if not body.startswith("SCORE FROM"):
        raise ValueError(f"unsupported rule, expected `SCORE FROM`: {body[:40]!r}")
    inner = body[body.index("(") + 1 : body.rindex(")")]

    groups = []
    for part in _split(inner, ","):
        text = part.strip()
        if not text:
            continue
        if text.startswith("MAX("):
            alternatives = [
                _item(alternative)
                for alternative in text[4 : text.rindex(")")].split(",")
                if alternative.strip()
            ]
        else:
            alternatives = [_item(text)]
        # Within a group the order of alternatives cannot change the
        # result, so sorting them makes the derived file canonical.
        groups.append(sorted(alternatives))
    return groups


def derive(xml: Path) -> str:
    """Derive the `.ruleset` text for one vendored ASI algorithm."""
    root = ElementTree.parse(xml).getroot()
    if root.tag != "ALGORITHM":
        raise ValueError(f"{xml.name}: root element is <{root.tag}>, not <ALGORITHM>")
    definitions = root.find("DEFINITIONS")
    if definitions is None:
        raise ValueError(f"{xml.name}: no <DEFINITIONS>")

    lines = [
        f"#ligase-hivdb-ruleset {FORMAT}",
        f"#derived from {xml.name} by scripts/vendor_hivdb_algorithm.py",
        "#do not edit by hand",
        "\t".join(
            (
                "algorithm",
                _text(root, "ALGNAME"),
                _text(root, "ALGVERSION"),
                _text(root, "ALGDATE"),
            )
        ),
    ]

    for level in definitions.findall("LEVEL_DEFINITION"):
        lines.append(
            "\t".join(
                (
                    "level",
                    _text(level, "ORDER"),
                    _text(level, "SIR"),
                    _text(level, "ORIGINAL"),
                )
            )
        )

    lines.append("\t".join(("default", _text(definitions, "DEFAULT_LEVEL"))))

    ranges = definitions.find("GLOBALRANGE")
    if ranges is None or ranges.text is None:
        raise ValueError(f"{xml.name}: no <GLOBALRANGE>")
    for part in ranges.text.strip().strip("()").split(","):
        bounds, _, level_order = part.partition("=>")
        low, _, high = bounds.strip().partition(" TO ")
        lines.append(
            "\t".join(
                (
                    "range",
                    # `*` rather than a sentinel number: a range open at one
                    # end is open, and writing -2147483648 would invite
                    # someone to compare against it.
                    "*" if low.strip() == "-INF" else str(int(low)),
                    "*" if high.strip() == "INF" else str(int(high)),
                    str(int(level_order)),
                )
            )
        )

    for gene in definitions.findall("GENE_DEFINITION"):
        lines.append(
            "\t".join(("gene", _text(gene, "NAME"), _text(gene, "DRUGCLASSLIST")))
        )
    for drug_class in definitions.findall("DRUGCLASS"):
        lines.append(
            "\t".join(
                ("class", _text(drug_class, "NAME"), _text(drug_class, "DRUGLIST"))
            )
        )

    of_class = {
        drug.strip(): _text(drug_class, "NAME")
        for drug_class in definitions.findall("DRUGCLASS")
        for drug in _text(drug_class, "DRUGLIST").split(",")
    }

    for drug in root.findall("DRUG"):
        name = _text(drug, "NAME")
        if name not in of_class:
            raise ValueError(f"{xml.name}: drug {name} is in no drug class")
        lines.append("\t".join(("drug", name, _text(drug, "FULLNAME"), of_class[name])))

        rules = drug.findall("RULE")
        if len(rules) != 1:
            raise ValueError(f"{xml.name}: drug {name} has {len(rules)} rules, not 1")
        actions = rules[0].find("ACTIONS")
        if actions is None or actions.find(".//USE_GLOBALRANGE") is None:
            raise ValueError(f"{xml.name}: drug {name} does not use the global range")

        for index, group in enumerate(_groups(_text(rules[0], "CONDITION"))):
            for item in group:
                lines.append(
                    "\t".join(
                        (
                            "rule",
                            name,
                            str(index),
                            str(item.score),
                            "+".join(item.residues),
                        )
                    )
                )

    return "\n".join(lines) + "\n"


def _pins() -> list[dict[str, str]]:
    """Read `PROVENANCE.tsv`."""
    rows = PROVENANCE.read_text().splitlines()
    header = next(row for row in rows if not row.startswith("#")).split("\t")
    return [
        dict(zip(header, row.split("\t"), strict=True))
        for row in rows
        if row and not row.startswith("#") and not row.startswith(header[0] + "\t")
    ]


def main() -> int:
    """Re-derive every pinned algorithm, checking or writing the result."""
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="compare, do not write")
    group.add_argument("--write", action="store_true", help="write the derived files")
    arguments = parser.parse_args()

    failures = []
    for pin in _pins():
        xml = RULESETS / pin["file"]
        digest = hashlib.sha256(xml.read_bytes()).hexdigest()
        if digest != pin["sha256"]:
            failures.append(
                f"{pin['file']}: SHA-256 is {digest}, pinned as {pin['sha256']}"
            )
            continue

        derived = derive(xml)
        out = RULESETS / f"{xml.stem}.ruleset"
        if arguments.write:
            out.write_text(derived)
            print(f"wrote {out.relative_to(RULESETS.parents[2])}")
        elif not out.is_file():
            failures.append(f"{out.name}: missing; run --write")
        elif out.read_text() != derived:
            failures.append(f"{out.name}: differs from what {xml.name} derives")

    for failure in failures:
        print(f"error: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
