"""M6 acceptance: the resistance calls, against published ones.

`docs/milestones.md` signs M6 off when "resistance calls match published
results ... with discrepancies explained". This is that comparison, run on
the real pipeline over real reads.

## What is compared against what

PRJNA1218827 is the HIVGenoPipe study (Dotrang et al., *BMC
Bioinformatics* 2025;26:168). It published, as supplementary data, the
Stanford HIVdb reports for all thirty of its samples at four ambiguity
thresholds — a score, a level and the contributing mutations for each of
twenty-seven drugs, produced by Sierra 3.5.3 against HIVDB 9.8.

Those expectations are committed in
`crates/ligase-core/tests/fixtures/hivdb/expected.tsv`; the SRA metadata
maps each run accession to the sample identifier the paper uses, so
nothing here rests on reading a figure.

Three of the four fetched runs are clinical samples and the fourth is the
study's no-template control:

| Run | Sample | What it tests |
|---|---|---|
| SRR32217726 | Sample 3 | a mixture — K219KE, and L100LI lower down |
| SRR32217715 | Sample 4 | a strong multi-class positive, INSTI and capsid |
| SRR32217716 | Sample 30 | fully susceptible, a clean negative |
| SRR32217714 | — | no template; nothing should be assessed at all |

## The thresholds

The pipeline's `mixture` parameter decides what fraction of the reads an
allele must reach to count as present, which is the same question the
paper's `Amb2`, `Amb5` and `Amb15` reports answer at 2%, 5% and 15%. One
set of alignments therefore validates all three.

15% and 5% are asserted exactly: every score, every level, every mutation.
2% is where the agreement stops, and the discrepancies there are listed
below one by one with their causes, so that a change in any of them fails
this test rather than passing quietly.
"""

from __future__ import annotations

import importlib.util
import re
import shutil
from pathlib import Path
from types import ModuleType

import pytest

from tests.conftest import DATA, REPO_ROOT, unavailable

#: The published expectations, as `scripts/build_hivdb_fixture.py` left
#: them. Committed, so this test needs no network and no supplement.
EXPECTED = (
    REPO_ROOT
    / "crates"
    / "ligase-core"
    / "tests"
    / "fixtures"
    / "hivdb"
    / "expected.tsv"
)

#: The demo pipeline, loaded by path.
DEMO = REPO_ROOT / "examples" / "hiv-resistance" / "pipeline.py"

#: The project whose samples have published resistance calls.
PROJECT = "PRJNA1218827"

#: What the demo shells out to.
TOOLS = ("minimap2", "samtools", "bcftools")

#: Run accession to the sample identifier the paper uses, from the SRA
#: metadata. `None` is the study's no-template control, which the paper
#: did not interpret and which should produce no assessment at all.
RUNS = {
    "SRR32217726": "Sample_3",
    "SRR32217715": "Sample_4",
    "SRR32217716": "Sample_30",
    "SRR32217714": None,
}

#: The published thresholds, and the `mixture` that corresponds to each.
THRESHOLDS = {"Amb15": 0.15, "Amb5": 0.05, "Amb2": 0.02}

#: The algorithm the published results were produced with. Pinned, because
#: a newer algorithm is *expected* to disagree with a 2025 answer, and that
#: disagreement would be science rather than a regression.
ALGORITHM = "HIVDB_9.8"

#: Every disagreement at 2%, with the cause established for each.
#:
#: Keyed by `(sample, drug)`. The value is what this pipeline says, as
#: `(score, level, mutations)`, beside the reason it differs. Listing them
#: rather than tolerating them is the point: a discrepancy that changes
#: shape fails here, and a new one fails here too.
KNOWN_AT_TWO_PERCENT = {
    # A codon is read from a VCF, which carries no phasing. At 2% both the
    # 45% G at HXB2 3204 and the 2.75% G at 3205 pass, and enumerating the
    # combinations yields AAA/AGA/GAA/GGA -- K, R, E and G -- although no
    # single read need carry both substitutions. Stanford's consensus for
    # this sample did not include the 3205 base, so it reports K and E
    # alone. Neither extra amino acid matches a rule, so every score and
    # level is identical; only the mutation string differs.
    ("Sample_3", "ABC"): ("5", "1", "K219KEGR"),
    ("Sample_3", "AZT"): ("10", "2", "K219KEGR"),
    ("Sample_3", "D4T"): ("10", "2", "K219KEGR"),
    ("Sample_3", "DDI"): ("5", "1", "K219KEGR"),
    ("Sample_3", "TDF"): ("5", "1", "K219KEGR"),
    # The same threshold effect on the reference allele: capsid 105 still
    # shows A in 2-5% of reads here, where Stanford's consensus had moved
    # wholly to T. Score and level identical.
    ("Sample_4", "LEN"): ("160", "5", "M66I,Q67H,N74D,A105AT"),
    # The one that changes a call, and the only false negative in the set.
    # Stanford reports V75VA; our alignment supports the V75A base in 3 of
    # 392 reads at HXB2 2773, which is 0.77% -- below 2%, so it is not
    # there to be found. HIVGenoPipe realigns against a sample-specific
    # reference, which is the paper's whole method and which places reads
    # this pipeline's alignment to HXB2 does not. A sensitivity limit at
    # the 1% level, not a disagreement about what the reads mean.
    ("Sample_30", "AZT"): ("0", "1", ""),
    ("Sample_30", "D4T"): ("0", "1", ""),
    ("Sample_30", "DDI"): ("0", "1", ""),
}


def _require_runnable() -> None:
    """Establish that the tools and this project's data are both here."""
    missing = [tool for tool in TOOLS if shutil.which(tool) is None]
    if missing:
        unavailable(f"not installed: {', '.join(missing)}")

    if not (DATA / "refs" / "HXB2.fasta").is_file():
        unavailable("data/refs/HXB2.fasta is missing; run scripts/fetch_test_data.sh")

    reads = DATA / "reads" / PROJECT
    for run in RUNS:
        if not (reads / f"{run}_1.fastq.gz").is_file():
            unavailable(
                f"{run} is missing; run scripts/fetch_test_data.sh --project {PROJECT}"
            )


@pytest.fixture(scope="module")
def demo() -> ModuleType:
    """The demo pipeline module, loaded from its path."""
    _require_runnable()
    specification = importlib.util.spec_from_file_location("ligase_demo", DEMO)
    assert specification is not None
    assert specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def cache(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """One cache for every threshold, so the reads are aligned once."""
    return tmp_path_factory.mktemp("validation")


def published() -> dict[tuple[str, str, str], tuple[str, str, str]]:
    """The published calls, by `(case, threshold, drug)`."""
    calls = {}
    for line in EXPECTED.read_text().splitlines():
        if line.startswith("#") or line.startswith("case\t") or not line.strip():
            continue
        case, drug, score, level, mutations = line.split("\t")
        sample, _, threshold = case.partition("_T1.")
        calls[sample, threshold, drug] = (score, level, mutations)
    return calls


#: What one run of the pipeline produced: sample, then drug, then
#: `(score, level, mutations)` exactly as the report writes them.
Calls = dict[str, dict[str, tuple[str, str, str]]]


def ours(demo: ModuleType, cache: Path, mixture: float) -> Calls:
    """Run the pipeline at one threshold and read back every report."""
    pipeline = demo.pipeline(
        runs=list(RUNS),
        project=PROJECT,
        mixture=mixture,
        algorithm=ALGORITHM,
    )
    pipeline.check()
    report = pipeline.run(cache=cache)

    calls: Calls = {}
    for step in report.steps:
        if step.step != "resistance":
            continue
        rows = Path(step.output).read_text().splitlines()
        calls[step.sample] = {
            fields[2]: (fields[4], fields[5], fields[7])
            for fields in (row.split("\t") for row in rows[1:])
        }
    return calls


def by_position(mutations: str) -> list[str]:
    """A mutation list in position order, whatever order it arrived in."""
    listed = [mutation for mutation in mutations.split(",") if mutation]
    return sorted(listed, key=lambda m: int(re.sub(r"\D", " ", m).split()[0]))


@pytest.mark.parametrize("threshold", ["Amb15", "Amb5"])
def test_the_calls_match_the_published_ones(
    demo: ModuleType, cache: Path, threshold: str
) -> None:
    """Every score, level and mutation, for every drug, for every sample.

    This is M6's acceptance criterion. Nothing is tolerated here: a
    difference of one point in one score on one drug fails it.
    """
    expected = published()
    calls = ours(demo, cache, THRESHOLDS[threshold])

    wrong = []
    checked = 0
    for run, sample in RUNS.items():
        if sample is None:
            continue
        for drug, (score, level, mutations) in calls[run].items():
            want = expected.get((sample, threshold, drug))
            assert want is not None, f"nothing published for {sample} {drug}"
            checked += 1
            if (score, level) != want[:2] or by_position(mutations) != by_position(
                want[2]
            ):
                wrong.append(
                    f"{sample} {drug}: ours {score}/{level} [{mutations}] "
                    f"vs published {want[0]}/{want[1]} [{want[2]}]"
                )

    assert checked == 81, f"{checked} calls compared, not 81"
    assert not wrong, f"{len(wrong)} of {checked} disagree:\n  " + "\n  ".join(wrong)


def test_the_remaining_discrepancies_are_the_explained_ones(
    demo: ModuleType, cache: Path
) -> None:
    """At 2% the agreement stops, in the places and for the reasons known.

    Every entry in `KNOWN_AT_TWO_PERCENT` carries its cause. A discrepancy
    that is not in that table, or one in it that has changed shape, fails
    here -- which is what stops "discrepancies explained" decaying into
    "discrepancies tolerated".
    """
    expected = published()
    calls = ours(demo, cache, THRESHOLDS["Amb2"])

    unexplained = []
    for run, sample in RUNS.items():
        if sample is None:
            continue
        for drug, got in calls[run].items():
            want = expected[sample, "Amb2", drug]
            agrees = got[:2] == want[:2] and by_position(got[2]) == by_position(want[2])
            known = KNOWN_AT_TWO_PERCENT.get((sample, drug))

            if agrees and known is not None:
                unexplained.append(
                    f"{sample} {drug} now agrees; remove it from KNOWN_AT_TWO_PERCENT"
                )
            elif not agrees and known is None:
                unexplained.append(
                    f"{sample} {drug}: ours {got} vs published {want}, "
                    "with no cause recorded"
                )
            elif not agrees and known != got:
                unexplained.append(
                    f"{sample} {drug}: ours {got}, but the recorded "
                    f"discrepancy is {known}"
                )

    assert not unexplained, "\n  ".join(["unexplained at 2%:", *unexplained])


def test_only_one_discrepancy_changes_a_clinical_call(
    demo: ModuleType, cache: Path
) -> None:
    """Only one mutation among the nine would change what anyone did.

    Worth asserting rather than describing. Six differ only in the mutation
    string while the score and the level are identical, so no clinician
    reading the report would act differently. The other three are one
    missed mutation counted once per drug it scores against: Sample 30's
    V75A, which this alignment supports in 3 of 392 reads and so cannot see
    at a 2% threshold. It moves three NRTIs off susceptible, and that is
    the honest cost of aligning to HXB2 rather than to a sample-specific
    reference.
    """
    expected = published()

    cosmetic: list[tuple[str, str]] = []
    clinical: list[tuple[str, str]] = []
    for (sample, drug), got in KNOWN_AT_TWO_PERCENT.items():
        want = expected[sample, "Amb2", drug]
        (cosmetic if got[:2] == want[:2] else clinical).append((sample, drug))

    assert len(cosmetic) == 6, cosmetic
    assert sorted(clinical) == [
        ("Sample_30", "AZT"),
        ("Sample_30", "D4T"),
        ("Sample_30", "DDI"),
    ], clinical


def test_the_negative_control_is_unassessed_rather_than_susceptible(
    demo: ModuleType, cache: Path
) -> None:
    """A no-template control must not read as a susceptible patient.

    This is the whole of why an uncovered position is not a wild-type
    position. Sixty of its thirty-two thousand reads map to HXB2, so there
    is no coverage anywhere; a pipeline that took "no variant called" to
    mean "agrees with the reference" would report this water sample as
    fully susceptible to all twenty-seven drugs.
    """
    calls = ours(demo, cache, THRESHOLDS["Amb15"])
    control = calls["SRR32217714"]

    assert len(control) == 27, control
    for drug, (score, level, _) in control.items():
        assert score == "", f"{drug} was scored on a no-template control"
        assert level == "", f"{drug} got a level on a no-template control"


def test_a_sample_with_coverage_is_assessed_on_every_drug(
    demo: ModuleType, cache: Path
) -> None:
    """The counterpart: a real sample gets an answer for all of them.

    Including lenacapavir, which is scored against capsid rather than pol.
    The demo's reads are a Gag-Pol amplicon and cover HXB2 752 to 5203, so
    capsid is genuinely there -- and Sample 4's published report carries a
    high-level lenacapavir call that this pipeline has to reproduce.
    """
    calls = ours(demo, cache, THRESHOLDS["Amb15"])
    sample_four = calls["SRR32217715"]

    assert len(sample_four) == 27
    assert all(score != "" for score, _, _ in sample_four.values())
    assert sample_four["LEN"][:2] == ("160", "5")
