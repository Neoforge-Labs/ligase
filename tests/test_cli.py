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
    # The sample is in the heading, because under fan-out the step name alone
    # does not say which of them failed.
    assert "DimensionError in step `call` (sample reads_phred33):" in out
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
