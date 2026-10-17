"""M2 acceptance: `ligase check` reports all type errors in one pass.

The pipeline file is imported, which is how a Python pipeline declares
itself. The step bodies are not run, so a wrong reference is caught before
anything reads a byte of sequence — and the test proves it by having each
body raise.
"""

from __future__ import annotations

import subprocess
import sys
from typing import TYPE_CHECKING

import pytest

from ligase.cli import main
from tests.conftest import FIXTURES

if TYPE_CHECKING:
    from pathlib import Path

PIPELINE = """
import ligase as lg
from ligase.types import Alignment, Reads, Reference, Variants

FIXTURES = {fixtures!r}


@lg.step(tool="minimap2")
def align(reads: Reads, ref: Reference) -> Alignment:
    raise AssertionError("a step body must never run during `ligase check`")


@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    raise AssertionError("a step body must never run during `ligase check`")


reads = lg.reads(f"{{FIXTURES}}/reads_phred33_1.fastq")
hxb2 = lg.reference(f"{{FIXTURES}}/{reference}", organism="HIV-1", build="tiny")
other = lg.reference(f"{{FIXTURES}}/{second}", organism="HIV-1", build="other")

pipe = lg.Pipeline(call(align(reads, hxb2), other))
"""


def write_pipeline(tmp_path: Path, reference: str, second: str) -> Path:
    path = tmp_path / "pipeline.py"
    path.write_text(
        PIPELINE.format(fixtures=str(FIXTURES), reference=reference, second=second)
    )
    return path


def test_a_clean_pipeline_checks_and_says_so(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = write_pipeline(tmp_path, "tiny_bare.fasta", "tiny_bare.fasta")

    assert main(["check", str(path)]) == 0

    out = capsys.readouterr().out
    assert "checked 2 steps in" in out
    assert "no problems found." in out


def test_a_wrong_reference_is_reported_without_running_anything(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = write_pipeline(tmp_path, "tiny_bare.fasta", "tiny_other.fasta")

    # The bodies raise if they run. A clean exit code of 1 means the error
    # was found from the signatures and the headers alone.
    assert main(["check", str(path)]) == 1

    out = capsys.readouterr().out
    # Exactly the shape `docs/golden-example.md` specifies. There is one
    # `call` step and no fan-out, so nothing is added to its heading: a
    # suffix identifying nothing is noise on the commonest path.
    assert "DimensionError in step `call`:\n  aln.reference = " in out
    assert "Did you pass the wrong reference?" in out
    assert "1 error in 1 step." in out


def test_the_default_pipeline_is_pipeline_py(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    write_pipeline(tmp_path, "tiny_bare.fasta", "tiny_bare.fasta")
    monkeypatch.chdir(tmp_path)

    assert main(["check"]) == 0
    assert "pipeline.py" in capsys.readouterr().out


def test_a_missing_file_is_not_a_passing_check(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert main(["check", str(tmp_path / "absent.py")]) == 2
    assert "no such pipeline" in capsys.readouterr().err


def test_a_file_with_no_pipeline_says_what_to_add(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = tmp_path / "pipeline.py"
    path.write_text("x = 1\n")

    assert main(["check", str(path)]) == 2
    assert "no lg.Pipeline found" in capsys.readouterr().err


def test_a_pipeline_that_will_not_import_is_reported_as_such(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = tmp_path / "pipeline.py"
    path.write_text("import ligase as lg\nraise RuntimeError('bad glob')\n")

    assert main(["check", str(path)]) == 2
    assert "could not be imported" in capsys.readouterr().err


def test_check_is_reachable_as_an_installed_command(tmp_path: Path) -> None:
    path = write_pipeline(tmp_path, "tiny_bare.fasta", "tiny_other.fasta")

    finished = subprocess.run(
        [sys.executable, "-m", "ligase", "check", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )

    assert finished.returncode == 1
    assert "DimensionError in step `call`" in finished.stdout


def test_an_unknown_command_is_a_usage_error() -> None:
    with pytest.raises(SystemExit) as caught:
        main(["sprint"])

    assert caught.value.code == 2


# ------------------------------------------------------- M4: `ligase audit` --

RUNNABLE = """
import ligase as lg
from ligase.types import Reference

FIXTURES = {fixtures!r}


@lg.step
def steady(ref: Reference) -> Reference:
    return lg.sh("cat {{ref}} > {{out}}")


@lg.step
def wobbling(ref: Reference) -> Reference:
    # The description records where it was written, as a `@PG CL` line does:
    # different bytes every time, the same sequence every time.
    return lg.sh("printf '>1 written to %s\\\\nACGT\\\\n' {{out}} > {{out}}")


hxb2 = lg.reference(f"{{FIXTURES}}/tiny_bare.fasta", organism="HIV-1", build="tiny")

pipe = lg.Pipeline([steady(hxb2), wobbling(hxb2)])
"""


def write_runnable(tmp_path: Path) -> Path:
    path = tmp_path / "pipeline.py"
    path.write_text(RUNNABLE.format(fixtures=str(FIXTURES)))
    return path


def test_auditing_a_reproducible_step_succeeds(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = write_runnable(tmp_path)

    code = main(["audit", "wobbling", str(path), "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert "auditing `wobbling`" in out
    # A step whose bytes move and whose meaning does not is not a finding,
    # so it is not a failing exit code either.
    assert code == 0, out
    assert "reproducible" in out
    assert "the bytes differ" in out


def test_auditing_a_deterministic_step_says_so(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = write_runnable(tmp_path)

    code = main(["audit", "steady", str(path), "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert code == 0, out
    assert "deterministic" in out
    assert "the same bytes both times" in out


def test_auditing_a_step_that_is_not_there_says_which_are(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = write_runnable(tmp_path)

    code = main(["audit", "stedy", str(path), "--cache", str(tmp_path / "cache")])

    # 2, not 1: this is a mistake in the command rather than a finding about
    # the pipeline.
    assert code == 2
    error = capsys.readouterr().err
    assert "no step named `stedy`" in error
    assert "`steady`" in error


def test_auditing_a_step_that_does_not_repeat_is_a_finding(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path = tmp_path / "pipeline.py"
    path.write_text(
        RUNNABLE.format(fixtures=str(FIXTURES))
        # A sequence nobody can predict: the shell's own process id.
        + "\n\n@lg.step\ndef drifting(ref: Reference) -> Reference:\n"
        '    return lg.sh("printf \'>1\\\\nACG%s\\\\n\' \\"$$\\" > {out}")\n\n'
        "drift = lg.Pipeline(drifting(hxb2))\n"
    )

    code = main(["audit", "drifting", str(path), "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    # 1, not 2: the pipeline is fine and the step is not.
    assert code == 1, out
    assert "not reproducible" in out
    assert "the two runs mean different things" in out
