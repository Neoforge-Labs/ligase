"""M18: several machines running one pipeline between them.

The Rust suite owns the mechanism — `crates/ligase-core/tests/cluster.rs`
puts two workers over one store and pins the lease, the stealing, the
divergence and the cache-directory rule. This file is about the surface a
user actually touches, and about the one thing threads in one process
cannot show: **real separate processes**, each with its own index, its own
local tier and no shared memory, coordinating through nothing but a
directory.

That matters because the whole claim is that the workers need no
coordinator. A test that shared a process would be quietly sharing a
mutex, a rayon pool and a page cache, which is three things a cluster does
not have.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest

from ligase import config
from ligase.cli import main
from tests.conftest import fixture

PIPELINE = """\
import ligase as lg
from ligase.types import Alignment, Reference


@lg.step(tool="awk")
def align(ref: Reference) -> Alignment:
    \"\"\"Stand in for an aligner: whatever it is, it is a shell command.\"\"\"
    return lg.sh("sleep 0.5; awk 'END {{ print NR }}' {ref} > {out}")


@lg.step(tool="awk")
def call(aln: Alignment) -> Alignment:
    \"\"\"A second step, so the graph has something below the first.\"\"\"
    return lg.sh("sleep 0.5; awk '{{ print $1 }}' {aln} > {out}")


ref = lg.reference(__FIXTURE__, organism="HIV-1", build="tiny")
pipeline = lg.Pipeline(call(align(ref)))
"""


@pytest.fixture(autouse=True)
def _own_store(monkeypatch: pytest.MonkeyPatch) -> None:
    """Leave `LIGASE_STORE` out of it, however the suite was invoked.

    These tests set the store themselves, per test, to their own
    directory. Inheriting CI's bucket would make two tests share a
    content-addressed namespace, which is two tests sharing their answers.

    `monkeypatch` rather than `os.environ`, because the variable has to be
    put *back*. Setting it and walking away is the M14 trap from the other
    end: a later test in this process asserts that a fresh cache directory
    means nothing has been computed, a leaked store makes that false, and
    the failure lands in a file that never heard of workers. Measured —
    six tests in four other modules, all of them reporting `cached` where
    they wanted `ran`.
    """
    monkeypatch.delenv(config.STORE_VARIABLE, raising=False)


def write_pipeline(where: Path) -> Path:
    """The two-step pipeline above, over a committed fixture."""
    path = where / "pipeline.py"
    # `replace` rather than `format`: the recipe holds `{ref}`, `{out}` and
    # the `{{ }}` an awk program needs, and formatting would eat them.
    path.write_text(
        PIPELINE.replace("__FIXTURE__", repr(str(fixture("tiny_bare.fasta"))))
    )
    return path


def worker(pipeline: Path, store: Path, name: str) -> subprocess.Popen[str]:
    """One `ligase run --worker` in a process of its own."""
    environment = dict(os.environ)
    environment[config.STORE_VARIABLE] = f"file://{store}"
    return subprocess.Popen(
        [
            sys.executable,
            "-m",
            "ligase",
            "run",
            str(pipeline),
            "--worker",
            name,
            "--jobs",
            "1",
            "--lease",
            "60",
        ],
        cwd=pipeline.parent,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )


# --------------------------------------------------- the whole thing works --


def test_two_processes_divide_one_pipeline_and_both_see_all_of_it(
    tmp_path: Path,
) -> None:
    """The milestone, end to end, with nothing shared but a directory."""
    pipeline = write_pipeline(tmp_path)
    store = tmp_path / "shared"
    store.mkdir()

    running = [worker(pipeline, store, name) for name in ("node01", "node02")]
    reports = []
    for process in running:
        out, _ = process.communicate(timeout=180)
        reports.append(out)
        assert process.returncode == 0, out

    # Each worker accounts for the whole graph, whether it computed a step
    # or took it from the store.
    for name, out in zip(("node01", "node02"), reports, strict=True):
        assert f"worker    {name}" in out, out
        assert "2 steps" in out, out

    # And between them each step ran once. The engine's own report gives a
    # step that ran its own line, led by `ran`, so counting those across
    # the two reports counts executions: two steps, two lines, not four.
    ran = sum(
        1
        for report in reports
        for line in report.splitlines()
        if line.startswith("ran ")
    )
    assert ran == 2, f"{ran} steps ran across two workers, not 2:\n\n" + "\n\n".join(
        reports
    )
    # One of them therefore waited for the other, with nothing between
    # them but a directory.
    assert any("waited for other workers" in report for report in reports), reports


def test_a_worker_takes_its_own_cache_directory(tmp_path: Path) -> None:
    """Two array tasks defaulting to one `$HOME/.ligase` must not collide.

    The index is SQLite, and SQLite over a network filesystem is the
    failure D43 names, so the right thing is the default rather than
    merely the documented thing.
    """
    pipeline = write_pipeline(tmp_path)
    store = tmp_path / "shared"
    store.mkdir()

    process = worker(pipeline, store, "node07")
    out, _ = process.communicate(timeout=180)
    assert process.returncode == 0, out

    assert (tmp_path / ".ligase" / "workers" / "node07").is_dir(), sorted(
        path.name for path in (tmp_path / ".ligase").iterdir()
    )
    # Not in the shared one, which is the whole point.
    assert not (tmp_path / ".ligase" / "cache.sqlite").exists()


def test_a_run_after_the_workers_collects_the_whole_graph(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The collector: one ordinary run, which is where a write-up comes from.

    Every step was computed on another worker, so every provenance record
    was adopted from the store rather than written here — which is what
    D43 fixed, used deliberately.
    """
    pipeline = write_pipeline(tmp_path)
    store = tmp_path / "shared"
    store.mkdir()

    process = worker(pipeline, store, "node01")
    out, _ = process.communicate(timeout=180)
    assert process.returncode == 0, out

    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{store}")
    code = main(["run", str(pipeline), "--cache", str(tmp_path / "desk")])
    assert code == 0

    # And it can write the run up, which a run that completed and could not
    # be written up is exactly what M14 found.
    code = main(["methods", str(pipeline), "--cache", str(tmp_path / "desk")])
    assert code == 0


# ------------------------------------------------------- what it refuses --


def test_a_worker_with_no_shared_store_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Workers divide work by claiming it somewhere they can all see.

    Without a store there is nobody to divide it with, and the run would
    quietly compute the whole pipeline alone and call itself a cluster.
    """
    pipeline = write_pipeline(tmp_path)

    code = main(["run", str(pipeline), "--worker", "node01"])

    # `ligase: ...` goes to stderr, which is the stream for saying so.
    said = capsys.readouterr()
    assert code != 0
    assert config.STORE_VARIABLE in said.err, said
    assert "shared filesystem" in said.err, said


def test_a_worker_name_that_is_a_path_is_refused(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A worker is named in its own cache directory, so `a/b` is a mistake.

    Reduced to a dash it would silently become a different worker from the
    one the user asked for, which for a name that selects a directory is
    worth a sentence rather than a shrug.
    """
    pipeline = write_pipeline(tmp_path)
    store = tmp_path / "shared"
    store.mkdir()
    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{store}")

    code = main(["run", str(pipeline), "--worker", "lab/node01"])

    said = capsys.readouterr()
    assert code != 0
    assert "path separator" in said.err, said
