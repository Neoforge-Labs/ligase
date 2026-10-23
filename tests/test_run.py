"""M3: running a pipeline, and not running what has not changed.

The Rust tests own the cache's behaviour over shell steps on text files.
These tests own the Python surface of it: that a body is asked once for a
recipe and never run as a step, that a parameter changed in a call reruns
what is below it, that a failure arrives as an exception carrying everything
that did work, and that nothing runs at all until the pipeline type-checks.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from ligase.types import Alignment, Reference, ResistanceReport, Variants
from tests.conftest import fixture


#: `awk` rather than the `wc` this used to be, and the reason is a defect
#: M13 fixed. The engine records `<tool> --version` as a tool's version, and
#: it used to ignore the exit status -- so on a BSD userland, where
#: `wc --version` prints `wc: illegal option` and exits non-zero, that
#: sentence *was* wc's recorded version. The same sentence for every release,
#: which is a cache key that cannot move when the tool does. `awk` answers
#: `--version` on both userlands, as a tool a step names has to.
@lg.step(tool="awk")
def align(ref: Reference) -> Alignment:
    """Stand in for an aligner: whatever it is, it is a shell command."""
    # The same number `wc -c` printed, so nothing downstream of this
    # stand-in had to change with it.
    return lg.sh("awk '{{ n += length($0) + 1 }} END {{ print n }}' {ref} > {out}")


@lg.step
def call(aln: Alignment, min_freq: float = 0.01) -> Variants:
    """Stand in for a variant caller, with a parameter that matters."""
    return lg.sh("sed s/^/{min_freq}:/ {aln} > {out}")


def _interpret(variants: str, out: Path) -> None:
    out.write_text(f"interpreted {Path(variants).name}\n")


@lg.step
def resistance(v: Variants) -> ResistanceReport:
    """A step whose work is Python rather than a command."""
    return lg.write(lambda out: _interpret(v.file, out))


@lg.step(tool="ligase-no-such-tool")
def needs_a_tool(ref: Reference) -> Alignment:
    """A step naming a tool that is not installed."""
    return lg.sh("cat {ref} > {out}")


@lg.step
def broken(ref: Reference) -> Alignment:
    """A step whose command fails."""
    return lg.sh("echo 'no such contig' >&2; exit 3")


@lg.step
def confused(ref: Reference) -> Alignment:
    """A step that does not say how its output is produced."""
    return ref  # type: ignore[return-value]


def reference(name: str = "tiny_bare.fasta") -> Reference:
    """A loaded reference, with the dimensions a FASTA cannot state."""
    return lg.reference(fixture(name), organism="HIV-1", build="tiny")


def outcomes(report: lg.RunReport) -> dict[str, str]:
    """What happened to each step, by name. One node per name here."""
    return {step.step: step.outcome for step in report.steps}


# ------------------------------------------------------------- running --


def test_a_pipeline_runs_and_reports_every_step(tmp_path: Path) -> None:
    pipe = lg.Pipeline(resistance(call(align(reference()))))

    report = pipe.run(cache=tmp_path)

    assert outcomes(report) == {"align": "ran", "call": "ran", "resistance": "ran"}
    assert report.summary == "3 steps: 3 ran."
    assert len(report.ran) == 3


def test_an_output_is_a_real_file_in_the_store(tmp_path: Path) -> None:
    pipe = lg.Pipeline(call(align(reference())))

    report = pipe.run(cache=tmp_path)
    [output] = report.outputs("call")

    assert Path(output).is_file()
    assert Path(output).read_text().startswith("0.01:")
    assert Path(output).is_relative_to(tmp_path)


def test_a_python_step_writes_its_own_output(tmp_path: Path) -> None:
    pipe = lg.Pipeline(resistance(call(align(reference()))))

    report = pipe.run(cache=tmp_path)
    [output] = report.outputs("resistance")

    assert Path(output).read_text().startswith("interpreted ")


def test_an_output_can_be_read_back_from_its_handle_after_a_run(
    tmp_path: Path,
) -> None:
    aligned = align(reference())
    pipe = lg.Pipeline(call(aligned))

    pipe.run(cache=tmp_path)

    assert Path(lg.handle(aligned).file).is_file()


def test_the_output_of_a_step_has_no_file_until_it_has_run() -> None:
    aligned = lg.handle(align(reference()))

    with pytest.raises(RuntimeError, match="has not produced anything yet"):
        _ = aligned.file


def test_loaded_data_knows_the_file_it_is() -> None:
    loaded = reference()

    assert loaded.file == loaded.path
    assert Path(loaded.file).is_file()


def test_data_with_no_file_behind_it_says_so_rather_than_returning_none() -> None:
    # A step's output carries dimensions worked out before anything ran, so
    # it has no file of its own until it has run.
    aligned = align(reference())
    lg.Pipeline(call(aligned))  # building the graph is what propagates them
    propagated = lg.handle(aligned).dataset
    assert propagated is not None

    assert propagated.path is None
    with pytest.raises(ValueError, match="no file behind it"):
        _ = propagated.file


# --------------------------------------------------------------- cache --


def test_a_second_run_comes_entirely_from_cache(tmp_path: Path) -> None:
    lg.Pipeline(resistance(call(align(reference())))).run(cache=tmp_path)

    again = lg.Pipeline(resistance(call(align(reference())))).run(cache=tmp_path)

    assert set(outcomes(again).values()) == {"cached"}
    assert again.summary == "3 steps: 3 from cache."


def test_a_second_run_does_not_reread_its_inputs(tmp_path: Path) -> None:
    # The prefilter: unchanged size and mtime means the file is not hashed
    # again, which is what makes content addressing affordable on a BAM.
    lg.Pipeline(align(reference())).run(cache=tmp_path)

    again = lg.Pipeline(align(reference())).run(cache=tmp_path)

    assert again.hashed == 0
    # Two files are reached for and neither is read: the reference the step
    # takes, and the output it is served from the cache, whose canonical
    # hash the report states.
    assert again.prefiltered == 2


def test_a_second_run_does_not_reparse_anything_either(tmp_path: Path) -> None:
    # What a file means is remembered against the hash of its bytes, so a
    # second run neither reads nor parses: it is the canonical hashes that
    # the keys are built from, and reparsing on every run would have made
    # semantic hashing cost a full pass over every BAM.
    lg.Pipeline(resistance(call(align(reference())))).run(cache=tmp_path)

    again = lg.Pipeline(resistance(call(align(reference())))).run(cache=tmp_path)

    assert again.canonicalised == 0


def test_changing_a_parameter_reruns_only_what_is_below_it(tmp_path: Path) -> None:
    lg.Pipeline(resistance(call(align(reference())))).run(cache=tmp_path)

    changed = lg.Pipeline(
        resistance(call(align(reference()), min_freq=0.05)),
    ).run(cache=tmp_path)

    assert outcomes(changed) == {
        "align": "cached",
        "call": "ran",
        "resistance": "ran",
    }


def test_an_added_sample_is_the_only_work(tmp_path: Path) -> None:
    first = reference("tiny_bare.fasta")
    second = reference("tiny_prefixed.fasta")
    lg.Pipeline([call(align(first)), call(align(second))]).run(cache=tmp_path)

    third = reference("tiny_other.fasta")
    report = lg.Pipeline(
        [call(align(first)), call(align(second)), call(align(third))],
    ).run(cache=tmp_path)

    ran = [step.node for step in report.ran]
    assert len(ran) == 2, report.text
    assert len(report.cached) == 4


def test_forcing_a_run_ignores_the_cache(tmp_path: Path) -> None:
    lg.Pipeline(call(align(reference()))).run(cache=tmp_path)

    forced = lg.Pipeline(call(align(reference()))).run(cache=tmp_path, force=True)

    assert set(outcomes(forced).values()) == {"ran"}


def test_a_run_defaults_to_a_cache_under_the_working_directory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)

    lg.Pipeline(align(reference())).run()

    assert (tmp_path / ".ligase" / "cache.sqlite").is_file()


# ------------------------------------------------------------ failures --


def test_a_failing_step_raises_with_the_whole_report(tmp_path: Path) -> None:
    pipe = lg.Pipeline([broken(reference()), align(reference())])

    with pytest.raises(lg.RunError) as raised:
        pipe.run(cache=tmp_path)

    report = raised.value.report
    assert [failure.step for failure in report.failures] == ["broken"]
    assert "no such contig" in report.failures[0].message
    # One sample failing does not take the others down with it.
    assert outcomes(report) == {"align": "ran"}
    assert "1 ran" in report.summary
    assert "1 failed" in report.summary


def test_a_missing_tool_is_reported_rather_than_ignored(tmp_path: Path) -> None:
    # Hashing an absent tool as "no version" would let two machines with
    # different tools agree on a key.
    pipe = lg.Pipeline(needs_a_tool(reference()))

    with pytest.raises(lg.RunError) as raised:
        pipe.run(cache=tmp_path)

    assert "ligase-no-such-tool" in raised.value.report.failures[0].message


def test_a_pipeline_that_does_not_check_does_not_run(tmp_path: Path) -> None:
    # `call` wants an Alignment and is handed a Reference.
    pipe = lg.Pipeline(call(reference()))  # type: ignore[arg-type]

    with pytest.raises(lg.StepTypeError):
        pipe.run(cache=tmp_path)

    assert not (tmp_path / "store").exists()


def test_a_body_that_does_not_say_how_to_produce_anything_is_refused(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(confused(reference()))

    with pytest.raises(lg.StepDefinitionError, match=r"lg\.sh"):
        pipe.run(cache=tmp_path)


# --------------------------------------------------------------- steps --


def test_a_step_can_name_several_tools() -> None:
    @lg.step(tool=("minimap2", "samtools"))
    def piped(ref: Reference) -> Alignment:
        return lg.sh("minimap2 {ref} | samtools sort -o {out}")

    assert piped.tools == ("minimap2", "samtools")


def test_a_step_that_names_no_tool_has_none() -> None:
    assert call.tools == ()
    assert align.tools == ("awk",)


def test_a_tool_named_as_an_empty_string_is_refused() -> None:
    with pytest.raises(ValueError, match="empty string"):

        @lg.step(tool="")
        def nameless(ref: Reference) -> Alignment:
            return lg.sh("cat {ref} > {out}")
