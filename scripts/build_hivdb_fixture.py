#!/usr/bin/env python3
"""Build the HIVdb scoring oracle from published Stanford HIVdb reports.

The scoring engine reads `crates/ligase-core/rulesets/HIVDB_9.8.ruleset`,
which is derived from `HIVDB_9.8.xml`. Checking it against expectations
derived from the same XML would test the parser against itself, so the
engine needs an oracle it does not share a source with.

That oracle is Stanford's own output. The HIVGenoPipe paper publishes, as
supplementary data, the Stanford HIVdb reports for thirty clinical samples
at four ambiguity thresholds -- a mutation list and, beside it, the score
and level HIVdb 9.8 assigned to each of twenty-seven drugs:

    Dotrang T, Sherman BT, Dai L, Khan MA, Highbarger HC, Bruchey W,
    Laverdure S, Baseler MW, Imamichi T, Dewar RL, Chang W. HIVGenoPipe: a
    nextflow pipeline for the detection of HIV-1 drug resistance using a
    real-time sample-specific reference sequence. BMC Bioinformatics
    2025;26:168. doi:10.1186/s12859-025-06201-5

    Additional file 1, "Supplemental Data 2-Example_Output.zip",
    `stanford_reports/`. Open access, CC BY 4.0.

Those reports were produced by Sierra 3.5.3 against HIVDB_9.8 -- a
different implementation, in a different language, by the algorithm's own
authors. Agreeing with them is evidence; agreeing with our own parse of the
XML would not be.

This script turns those reports into two committed fixtures. It is run by
hand, once, because its input is a 26 MB supplement that is not in this
repository and is not fetched by CI:

    scripts/build_hivdb_fixture.py --reports <unpacked>/stanford_reports

## What ends up in the fixtures

`cases.tsv` is the input to the scorer: for each case, the amino acids
present at each position of each protein. A position absent from a case
holds the subtype-B consensus amino acid and is left out, exactly as
Stanford's mutation list leaves it out.

Only positions that some rule in some vendored algorithm mentions are
kept. The published lists run to twenty-five thousand mutations, almost
all of them ordinary polymorphism at positions no rule looks at, and a
`SCORE FROM` rule cannot be influenced by a position it does not name --
so dropping them changes no expectation and takes the fixture from 710 KB
to a size worth reading. The full lists for the three samples the
validation uses are kept separately, where they are compared as mutation
lists rather than used as scorer input.

`expected.tsv` is what HIVdb 9.8 said: a score, a level and the
contributing mutations for every drug, for every case. Scores are integers
here although the reports write them as `-10.0`; HIVdb scores are whole
numbers, and reading them as integers is what stops a float comparison
creeping into a test that should be exact.

The mutations are re-sorted by position. Stanford lists them in the order
its rules matched, which carries no meaning a reader needs and would make
an otherwise identical answer compare unequal.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

#: Where the vendored rulesets live, so the script can tell which
#: positions a rule can actually look at.
RULESETS = Path(__file__).resolve().parents[1] / "crates" / "ligase-core" / "rulesets"

#: Where the fixtures are written.
FIXTURES = (
    Path(__file__).resolve().parents[1]
    / "crates"
    / "ligase-core"
    / "tests"
    / "fixtures"
    / "hivdb"
)

#: The algorithm version these reports were produced with. A report from
#: any other version is refused rather than mixed in: a fixture holding two
#: algorithms would not say which one an expectation came from.
ALGORITHM = "HIVDB_9.8"

#: The header both fixtures carry, naming where the data came from. A
#: fixture whose provenance lives only in a commit message is a fixture
#: nobody can check.
CREDIT = (
    "#Stanford HIVdb {algorithm} results, as published in the supplementary\n"
    "#data of Dotrang et al., BMC Bioinformatics 2025;26:168\n"
    "#(doi:10.1186/s12859-025-06201-5), Additional file 1, Supplemental\n"
    "#Data 2, stanford_reports/. Open access, CC BY 4.0.\n"
    "#\n"
    "#Produced by Sierra 3.5.3 -- an independent implementation of the same\n"
    "#algorithm, which is what makes these worth testing against.\n"
    "#Built by scripts/build_hivdb_fixture.py; do not edit by hand.\n"
)


def _scored_positions() -> set[tuple[str, int]]:
    """Every `(gene, position)` any vendored algorithm has a rule about."""
    of_drug: dict[str, str] = {}
    positions: set[tuple[str, int]] = set()
    pending: list[tuple[str, int]] = []

    for ruleset in sorted(RULESETS.glob("*.ruleset")):
        classes: dict[str, str] = {}
        for line in ruleset.read_text().splitlines():
            if line.startswith("#"):
                continue
            kind, *fields = line.split("\t")
            if kind == "gene":
                for name in fields[1].split(","):
                    classes[name] = fields[0]
            elif kind == "drug":
                of_drug[fields[0]] = classes[fields[2]]
            elif kind == "rule":
                for residue in fields[3].split("+"):
                    matched = re.match(r"\d+", residue)
                    if matched is None:
                        raise ValueError(f"{ruleset.name}: bad residue {residue!r}")
                    pending.append((fields[0], int(matched.group())))

    for drug, position in pending:
        positions.add((of_drug[drug], position))
    return positions


def _rows(path: Path) -> list[dict[str, str]]:
    """Read a tab-separated report into dictionaries."""
    lines = path.read_text().splitlines()
    header = lines[0].split("\t")
    return [
        dict(zip(header, line.split("\t"), strict=False))
        for line in lines[1:]
        if line.strip()
    ]


def main() -> int:
    """Build both fixtures from a directory of Stanford reports."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--reports",
        type=Path,
        required=True,
        help="the unpacked stanford_reports/ directory",
    )
    arguments = parser.parse_args()

    scored = _scored_positions()
    cases: list[tuple[str, str, int, str]] = []
    expected: list[tuple[str, str, int, int, str]] = []

    for resistance in sorted(arguments.reports.glob("*.Drug_Resistance.tsv")):
        case = resistance.name.removesuffix(".Drug_Resistance.tsv")
        mutations = resistance.with_name(f"{case}.Mutations.tsv")
        if not mutations.is_file():
            print(f"error: no mutations beside {resistance.name}", file=sys.stderr)
            return 1

        drugs = _rows(resistance)
        versions = {row["hivdb_version"] for row in drugs}
        if versions != {ALGORITHM}:
            print(
                f"error: {resistance.name} is {sorted(versions)}, not {ALGORITHM}",
                file=sys.stderr,
            )
            return 1

        for row in _rows(mutations):
            # The published `mutation` column holds every amino acid at the
            # position, the reference one included when the sample is a
            # mixture -- `KE` at RT 219 is a K/E mixture. That set is
            # exactly what a rule is matched against.
            site = int(row["mutation_site"])
            if (row["protein"], site) in scored:
                cases.append((case, row["protein"], site, row["mutation"]))

        for row in drugs:
            score = float(row["score"])
            if score != int(score):
                print(
                    f"error: {case} {row['drug_abbr']} scores {score}",
                    file=sys.stderr,
                )
                return 1
            mutations = [
                mutation.strip()
                for mutation in row["mutations"].split(",")
                if mutation.strip()
            ]
            mutations.sort(
                key=lambda mutation: int(re.sub(r"\D", " ", mutation).split()[0])
            )
            expected.append(
                (
                    case,
                    row["drug_abbr"],
                    int(score),
                    int(row["level"]),
                    ",".join(mutations),
                )
            )

    FIXTURES.mkdir(parents=True, exist_ok=True)
    credit = CREDIT.format(algorithm=ALGORITHM)

    (FIXTURES / "cases.tsv").write_text(
        credit
        + "case\tprotein\tposition\tamino_acids\n"
        + "".join(
            f"{case}\t{protein}\t{site}\t{acids}\n"
            for case, protein, site, acids in sorted(cases)
        )
    )
    (FIXTURES / "expected.tsv").write_text(
        credit
        + "case\tdrug\tscore\tlevel\tmutations\n"
        + "".join(
            f"{case}\t{drug}\t{score}\t{level}\t{mutations}\n"
            for case, drug, score, level, mutations in sorted(expected)
        )
    )

    print(f"{len(cases)} mutations and {len(expected)} expectations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
