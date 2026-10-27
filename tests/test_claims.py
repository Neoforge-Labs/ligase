"""M16: the Python surface of claim binding.

The Rust tests own what a claim binds to and what the write-up says about
one. These own what Python sees: that a claim is declared on the pipeline and
so reaches `ligase methods` with no new flag, that a declaration the engine
could not check is refused where it is written, that an unbacked number makes
a write-up not accounted for, and that `ligase methods` exits non-zero on
one — which is the only place a drifting manuscript is actually caught.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

import ligase as lg
from ligase.provenance import BOUND, CONTRADICTED, UNBACKED
from ligase.types import Alignment, Reference
from tests.conftest import fixture


@lg.step
def copy(ref: Reference) -> Alignment:
    """A step that copies its input."""
    return lg.sh("cat {ref} > {out}")


@lg.step
def call(ref: Reference, min_freq: float = 0.01) -> Alignment:
    """A step with a threshold a paper would restate."""
    return lg.sh("cat {ref} > {out} && echo {min_freq} > /dev/null")


@lg.step(tool="python3")
def align(ref: Reference) -> Alignment:
    """A step that runs a tool whose version a paper would state."""
    return lg.sh("cat {ref} > {out}")


def reference(name: str = "tiny_bare.fasta") -> Reference:
    """A loaded reference, with the dimensions a FASTA cannot state."""
    return lg.reference(fixture(name), organism="HIV-1", build="tiny")


# ------------------------------------------- what a declaration must say --


def test_a_claim_that_names_both_a_parameter_and_a_tool_is_refused() -> None:
    with pytest.raises(ValueError, match="restates one thing"):
        lg.Claim(
            "called above 1% with minimap2 2.31",
            step="call",
            parameter="min_freq",
            tool="minimap2",
            value="0.01",
        )


def test_a_value_with_nothing_to_compare_it_against_is_refused() -> None:
    with pytest.raises(ValueError, match="names nothing it is the value of"):
        lg.Claim("called above 1%", step="call", value="0.01")


def test_a_parameter_with_no_value_is_refused() -> None:
    with pytest.raises(ValueError, match="nothing to compare it against"):
        lg.Claim("called above 1%", step="call", parameter="min_freq")


def test_a_claim_with_no_step_is_refused() -> None:
    with pytest.raises(ValueError, match="names no step"):
        lg.Claim("three variants were called", step="")


def test_a_claim_with_no_sentence_is_refused() -> None:
    with pytest.raises(ValueError, match="the sentence the paper states"):
        lg.Claim("   ", step="call")


def test_a_claim_resting_on_a_step_alone_is_a_complete_declaration() -> None:
    claim = lg.Claim("every sample was copied", step="copy")

    assert claim.rendered is None
    assert claim.scope is None
    assert "every sample was copied" in repr(claim)


def test_a_value_is_rendered_the_way_the_parameter_was_hashed() -> None:
    def rendered(value: str | float) -> str | None:
        return lg.Claim("1%", step="call", parameter="min_freq", value=value).rendered

    assert rendered(0.01) == "0.01"
    assert rendered("0.01") == "0.01"


# ------------------------------------------------------- what is bound --


def test_a_claim_is_bound_to_the_step_that_produced_it(tmp_path: Path) -> None:
    pipe = lg.Pipeline(
        copy(reference()),
        claims=[lg.Claim("the reference was copied once", step="copy")],
    )

    methods = pipe.methods(cache=tmp_path)

    assert [check.verdict for check in methods.claims] == [BOUND]
    check = methods.claims[0]
    assert check.is_bound
    assert len(check.bound) == 1
    assert len(check.bound[0].key) == 64, "the whole key, not the sentence's 16"
    assert check.bound[0].canonical
    assert check.bound[0].platform
    assert methods.accounted_for
    assert methods.unbacked == []


def test_a_claim_restating_a_parameter_the_run_agrees_with_is_bound(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        call(reference(), min_freq=0.01),
        claims=[
            lg.Claim(
                "variants were called above 1% allele frequency",
                step="call",
                parameter="min_freq",
                value=0.01,
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)

    assert methods.claims[0].verdict == BOUND
    assert methods.claims[0].recorded is None


def test_the_pipeline_keeps_the_claims_as_they_were_written() -> None:
    claims = [lg.Claim("the reference was copied once", step="copy")]

    pipe = lg.Pipeline(copy(reference()), claims=claims)

    assert pipe.claims == tuple(claims)


# ---------------------------------------------------------- contradicted --


def test_a_claim_restating_a_parameter_the_run_disagrees_with_is_contradicted(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        call(reference(), min_freq=0.05),
        claims=[
            lg.Claim(
                "variants were called above 1% allele frequency",
                step="call",
                parameter="min_freq",
                value=0.01,
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)
    check = methods.claims[0]

    assert check.verdict == CONTRADICTED
    assert check.recorded == "0.05", "what the run did, beside what the paper says"
    assert not check.is_bound
    assert not methods.accounted_for, (
        "a number the analysis positively disagrees with is a hole in the write-up"
    )
    assert "0.01" in check.reason
    assert "0.05" in check.reason
    assert "0.05" in methods.text


def test_a_claim_restating_a_tool_version_is_checked_against_what_ran(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        align(reference()),
        claims=[
            lg.Claim(
                "reads were aligned under Python 2.7",
                step="align",
                tool="python3",
                value="2.7",
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)
    check = methods.claims[0]

    assert check.verdict == CONTRADICTED
    assert check.recorded is not None
    assert "2.7" in check.reason


def test_a_tool_version_the_run_agrees_with_is_bound(tmp_path: Path) -> None:
    # Whatever this machine's python3 is, the claim states its real version,
    # so the check is about the comparison rather than about the number.
    running = f"{sys.version_info.major}.{sys.version_info.minor}"
    pipe = lg.Pipeline(
        align(reference()),
        claims=[
            lg.Claim(
                f"reads were aligned under Python {running}",
                step="align",
                tool="python3",
                value=running,
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)

    assert methods.claims[0].verdict == BOUND, methods.claims[0].reason


# -------------------------------------------------------------- unbacked --


def test_a_claim_naming_a_step_the_pipeline_does_not_run_is_unbacked(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        copy(reference()),
        claims=[lg.Claim("three resistance mutations were found", step="resistance")],
    )

    methods = pipe.methods(cache=tmp_path)
    check = methods.claims[0]

    assert check.verdict == UNBACKED
    assert check.bound == [], "nothing stands behind it"
    assert "produces `copy`" in check.reason, (
        "the complaint names what the pipeline does produce, so a renamed "
        "step is a one-line fix"
    )
    assert not methods.accounted_for
    assert methods.unbacked == [check]


def test_a_claim_naming_a_parameter_the_step_does_not_take_is_unbacked(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        call(reference()),
        claims=[
            lg.Claim(
                "variants were called above 1%",
                step="call",
                parameter="min_frequency",
                value="0.01",
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)

    assert methods.claims[0].verdict == UNBACKED
    assert "`min_freq`" in methods.claims[0].reason


def test_a_claim_naming_a_tool_the_step_did_not_run_is_unbacked(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(
        align(reference()),
        claims=[
            lg.Claim(
                "reads were aligned with bwa 0.7.18",
                step="align",
                tool="bwa",
                value="0.7.18",
            )
        ],
    )

    methods = pipe.methods(cache=tmp_path)

    assert methods.claims[0].verdict == UNBACKED
    assert "`python3`" in methods.claims[0].reason


# -------------------------------------------------- nothing else changes --


def test_a_pipeline_that_declares_no_claims_is_unchanged(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))

    methods = pipe.methods(cache=tmp_path)

    assert methods.claims == []
    assert methods.unbacked == []
    assert methods.accounted_for
    assert "Claims" not in methods.text, (
        "a heading over an empty section is a promise nobody made"
    )
    assert methods.summary == "1 step, all accounted for."


def test_a_claim_is_in_no_cache_key(tmp_path: Path) -> None:
    first = lg.Pipeline(copy(reference()))
    first.methods(cache=tmp_path)

    # The manuscript is written, and the pipeline is otherwise untouched.
    second = lg.Pipeline(
        copy(reference()),
        claims=[lg.Claim("the reference was copied once", step="copy")],
    )
    report = second.run(cache=tmp_path)

    assert [run.outcome for run in report.steps] == ["cached"], (
        "editing a manuscript must rerun nothing"
    )


# ---------------------------------------------------------- the bundle --


def test_claims_travel_in_the_bundle(tmp_path: Path) -> None:
    pipe = lg.Pipeline(
        copy(reference()),
        claims=[lg.Claim("the reference was copied once", step="copy")],
    )

    methods = pipe.methods(cache=tmp_path, bundle=tmp_path / "bundle")

    assert methods.bundle is not None
    assert "claims.tsv" in methods.bundle.files
    table = (tmp_path / "bundle" / "claims.tsv").read_text()
    assert "the reference was copied once" in table
    assert methods.claims[0].bound[0].key in table, "the whole key, to look up"


# ------------------------------------------------------- the command line --


PIPELINE = '''import ligase as lg
from ligase.types import Alignment, Reference

ref = lg.reference({fixture!r}, organism="HIV-1", build="tiny")


@lg.step
def copy(ref: Reference) -> Alignment:
    """A step that copies its input."""
    return lg.sh("cat {{ref}} > {{out}}")


pipe = lg.Pipeline(
    copy(ref),
    claims=[lg.Claim({claim!r}, step={step!r})],
)
'''


def run_cli(pipeline: Path, cache: Path) -> subprocess.CompletedProcess[str]:
    """`ligase methods` on a pipeline file, as a user runs it."""
    return subprocess.run(
        [
            sys.executable,
            "-m",
            "ligase",
            "methods",
            str(pipeline),
            "--cache",
            str(cache),
        ],
        capture_output=True,
        text=True,
        check=False,
    )


def test_ligase_methods_finds_claims_on_the_pipeline_with_no_new_flag(
    tmp_path: Path,
) -> None:
    pipeline = tmp_path / "pipeline.py"
    pipeline.write_text(
        PIPELINE.format(
            fixture=str(fixture("tiny_bare.fasta")),
            claim="the reference was copied once",
            step="copy",
        )
    )

    done = run_cli(pipeline, tmp_path / "cache")

    assert done.returncode == 0, done.stdout + done.stderr
    assert "Claims" in done.stdout
    assert "the reference was copied once" in done.stdout


def test_ligase_methods_exits_non_zero_on_an_unbacked_claim(tmp_path: Path) -> None:
    pipeline = tmp_path / "pipeline.py"
    pipeline.write_text(
        PIPELINE.format(
            fixture=str(fixture("tiny_bare.fasta")),
            claim="three resistance mutations were found",
            step="resistance",
        )
    )

    done = run_cli(pipeline, tmp_path / "cache")

    assert done.returncode != 0, (
        "CI is the only place a drifting manuscript is caught, so an "
        "unbacked claim has to fail there"
    )
    assert "has nothing behind it" in done.stdout
