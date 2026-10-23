"""M14: `ligase store`, and where the store URL is allowed to come from.

The interesting assertions here are about what the output *says* rather
than what it does. "How many objects are local only" is a difference
between two stores, which is not free to compute, so the number is exact
up to a bound and sampled above it — and a user who reads a sampled number
as an exact one has been misled by a command that was trying to be fast.

And `ligase.toml` is a file people commit, so the one place a user would
try to put a secret is the one place that has to refuse it.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from ligase import config
from ligase.cli import main
from tests.conftest import fixture


@pytest.fixture(autouse=True)
def _no_inherited_store() -> None:
    """Leave `LIGASE_STORE` out of it, however the suite was invoked.

    CI runs the whole suite against a MinIO container with the variable
    set. These tests are about the variable itself, so they set it or they
    unset it; inheriting one would make them test the runner.
    """
    os.environ.pop(config.STORE_VARIABLE, None)


def write_pipeline(where: Path) -> Path:
    """A one-step pipeline over a committed fixture."""
    path = where / "pipeline.py"
    path.write_text(
        "import ligase as lg\n"
        "from ligase.types import Alignment, Reference\n"
        "\n"
        "@lg.step(tool='wc')\n"
        "def align(ref: Reference) -> Alignment:\n"
        '    """Stand in for an aligner."""\n'
        '    return lg.sh("wc -c < {ref} > {out}")\n'
        "\n"
        f"ref = lg.reference({str(fixture('tiny_bare.fasta'))!r},"
        " organism='HIV-1', build='tiny')\n"
        "pipeline = lg.Pipeline(align(ref))\n"
    )
    return path


# ------------------------------------------------------------------ status --


def test_status_says_when_the_build_has_no_remote_support(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    code = main(["store", "status", "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert code == 0, out
    assert "remote-store support" in out
    # Whichever way this build was compiled, the line has to be decidable
    # from the output alone: that is the done-when clause.
    assert ("support   yes" in out) or ("support   no" in out)


def test_status_with_nothing_configured_says_how_to_configure_it(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    main(["store", "status", "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert "none configured" in out
    assert config.STORE_VARIABLE in out
    assert config.CONFIG_FILE in out


def test_status_names_where_the_url_came_from(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    (tmp_path / "ligase.toml").write_text(f'[store]\nurl = "file://{bucket}"\n')
    monkeypatch.chdir(tmp_path)

    main(["store", "status", "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert "ligase.toml" in out, out
    assert str(bucket) in out


def test_the_environment_beats_the_file(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    # So that a one-off run against another bucket is a prefix on a command
    # rather than an edit to a file under version control.
    (tmp_path / "ligase.toml").write_text('[store]\nurl = "file:///from/the/file"\n')
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{tmp_path / 'bucket'}")

    main(["store", "status", "--cache", str(tmp_path / "cache")])

    out = capsys.readouterr().out
    assert config.STORE_VARIABLE in out
    assert "/from/the/file" not in out


def test_status_counts_local_only_and_says_how_it_counted(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{bucket}")
    path = write_pipeline(tmp_path)
    cache = tmp_path / "cache"
    assert main(["run", str(path), "--cache", str(cache)]) == 0
    capsys.readouterr()

    main(["store", "status", "--cache", str(cache)])

    out = capsys.readouterr().out
    assert "reachable              yes" in out, out
    # A run writes through, so everything local is in the bucket too.
    assert "local only             0 (exact)" in out, out


def test_a_small_store_is_counted_exactly(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    # The word matters as much as the number: a cheap number read as an
    # exact one is its own kind of wrong answer.
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{bucket}")

    main(["store", "status", "--cache", str(tmp_path / "cache")])

    assert "(exact)" in capsys.readouterr().out


# ----------------------------------------------------------- push and pull --


def test_push_then_pull_moves_objects(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    path = write_pipeline(tmp_path)
    first = tmp_path / "first"

    # Run with no store at all, so nothing is written through.
    assert main(["run", str(path), "--cache", str(first)]) == 0
    capsys.readouterr()

    # Then configure one and make up the push that never happened.
    monkeypatch.setenv(config.STORE_VARIABLE, f"file://{bucket}")
    assert main(["store", "push", "--cache", str(first)]) == 0
    pushed = capsys.readouterr().out
    assert "pushed" in pushed
    assert "0 objects" not in pushed, pushed

    # A second checkout, with the index copied across but no objects: pull
    # fetches what the index refers to.
    second = tmp_path / "second"
    second.mkdir()
    for name in ("cache.sqlite", "provenance.sqlite"):
        if (first / name).is_file():
            (second / name).write_bytes((first / name).read_bytes())

    assert main(["store", "pull", "--cache", str(second)]) == 0
    out = capsys.readouterr().out
    assert "fetched" in out
    assert "0 objects" not in out, out


def test_push_with_no_store_says_so(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    code = main(["store", "push", "--cache", str(tmp_path / "cache")])

    assert code == 2
    assert config.STORE_VARIABLE in capsys.readouterr().err


# -------------------------------------------------------------- the config --


def test_a_secret_in_ligase_toml_is_refused(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    (tmp_path / "ligase.toml").write_text(
        '[store]\nurl = "s3://lab"\nsecret_access_key = "AKIAIOSFODNN7EXAMPLE"\n'
    )
    monkeypatch.chdir(tmp_path)

    code = main(["store", "status"])

    err = capsys.readouterr().err
    assert code == 2
    assert "secret_access_key" in err
    assert "chain" in err, err


def test_an_unknown_key_in_the_store_table_is_refused(tmp_path: Path) -> None:
    (tmp_path / "ligase.toml").write_text('[store]\nurl = "s3://lab"\nregion = "eu"\n')

    with pytest.raises(config.ConfigError, match="region"):
        config.store(tmp_path)


def test_the_config_is_found_from_a_subdirectory(tmp_path: Path) -> None:
    # One file at the root of a repository serves every pipeline in it.
    (tmp_path / "ligase.toml").write_text('[store]\nurl = "s3://lab"\n')
    deep = tmp_path / "analyses" / "cohort"
    deep.mkdir(parents=True)

    found = config.store(deep)

    assert found is not None
    assert found.url == "s3://lab"


def test_a_toml_with_no_store_table_is_not_a_store(tmp_path: Path) -> None:
    (tmp_path / "ligase.toml").write_text("[tool.something]\nvalue = 1\n")

    assert config.store(tmp_path) is None


def test_a_broken_toml_is_reported_rather_than_ignored(tmp_path: Path) -> None:
    (tmp_path / "ligase.toml").write_text("[store\nurl =")

    with pytest.raises(config.ConfigError):
        config.store(tmp_path)


# ------------------------------------------------- two machines, one bucket --

#: A bucket to run the end-to-end check against, set by CI's `store` job.
#:
#: These skip rather than fall back to a directory. A `file://` store
#: already has its own tests; the point of these is the real backend, and a
#: test that silently checks something else is worse than one that says it
#: did not run.
TEST_STORE = os.environ.get("LIGASE_TEST_STORE", "").strip()

needs_a_bucket = pytest.mark.skipif(
    not TEST_STORE, reason="LIGASE_TEST_STORE names no bucket"
)


@needs_a_bucket
def test_a_second_machine_runs_nothing(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    """The done-when clause, as a user would meet it.

    One machine computes a pipeline; a second, with nothing of its own but
    the same bucket, completes it without running a step. The outputs have
    to be real files on the second machine, because the step below one is
    handed a path.
    """
    monkeypatch.setenv(config.STORE_VARIABLE, f"{TEST_STORE}/two-machines")
    path = write_pipeline(tmp_path)

    assert main(["run", str(path), "--cache", str(tmp_path / "first")]) == 0
    first = capsys.readouterr().out
    assert "1 ran" in first, first

    assert main(["run", str(path), "--cache", str(tmp_path / "second")]) == 0
    second = capsys.readouterr().out
    assert "1 ran" not in second, second
    assert "from cache" in second, second
    assert "from the remote store" in second, "and the report says where it came from"


@needs_a_bucket
def test_a_second_machine_can_write_up_what_it_did_not_run(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    """Provenance travels with the entry, or the results cannot be reported.

    Without this the headline scenario -- one person computes, everybody
    else hits the cache -- produces a methods paragraph that says "is in
    the cache from a run that recorded no provenance" for every step.
    """
    monkeypatch.setenv(config.STORE_VARIABLE, f"{TEST_STORE}/write-up")
    path = write_pipeline(tmp_path)

    assert main(["run", str(path), "--cache", str(tmp_path / "first")]) == 0
    capsys.readouterr()

    assert main(["methods", str(path), "--cache", str(tmp_path / "second")]) == 0
    out = capsys.readouterr().out
    assert "recorded no provenance" not in out, out
    assert "wc" in out, "the tool it was produced with is named: " + out
