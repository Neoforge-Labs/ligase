#!/usr/bin/env python3
"""Capture the engine's real diagnostics into a module the site imports.

The pages that describe a silent failure claim to show what Ligase actually
says about it. Pasting that text would let it drift the moment the wording
changed, so it is captured from a live run on every docs build instead.

That makes this script a test as well as a generator. Each scenario declares
the error type it expects, and the build fails if the engine produces nothing,
produces something else, or produces more than one error -- any of which would
mean either a regression in the engine or a page making a claim that is no
longer true.

The inputs are the committed test fixtures, by their real paths, so the file
names in the captured output are the ones a reader can go and look at. The
reference-build scenario declares `build=` names, which is how a real pipeline
names a build the file headers cannot identify on their own.

Most scenarios are one step's inputs disagreeing with each other, which
`lg.dimension_errors` finds on its own. Two are not. A reference in the wrong
coordinate frame disagrees with what the *step* reads, which is known from
its return annotation rather than from any input, so that one is checked as a
whole pipeline (`pipeline=True`). And a remote input is refused before there
is a step at all -- a digest that does not match fails while the pipeline
file is still being read -- so those scenarios name a `provoke` function and
the error it must raise.

The remote scenarios serve their fixture from a local HTTP server on a
loopback port, which is a scheme the engine accepts for exactly this reason:
TLS protects a channel against something sitting in it, and between two ends
of a loopback socket there is nothing to sit in. So these are real fetches
over a real socket, and the docs build still reaches no network.
"""

from __future__ import annotations

import json
import os
import sys
import threading
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    # Only for the type checker's sake, and only for the three scenarios that
    # define a step. `@lg.step` resolves a step's annotations with
    # `get_type_hints`, which reads the *defining module's* globals and not
    # the enclosing function's locals -- so those scenarios put the real
    # classes into this module's globals as they run, and a kind named only
    # inside one of them would otherwise fail to resolve and the captured
    # diagnostic would be about that instead of about the seed.
    from ligase.types import RankedList, Reads

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "crates/ligase-core/tests/fixtures"
OUT = Path(__file__).resolve().parents[1] / "lib/diagnostics.generated.ts"


@dataclass(frozen=True)
class Scenario:
    """One silent failure, and the error the engine must raise for it."""

    key: str
    """The identifier the MDX imports."""
    step: str
    """The step name the diagnostic is reported against."""
    expect: str
    """The error class name the engine must produce."""
    description: str
    """What this demonstrates, for the generated file's comments."""
    build: Callable[[Any], dict[str, Any]] | None = None
    """Returns the step's named inputs, given the `ligase` module."""
    pipeline: bool = False
    """Check a whole one-step pipeline rather than the inputs alone.

    For a disagreement between an input and what the step itself reads,
    which `dimension_errors` cannot see: it compares inputs, and a step's
    own coordinate frame comes from its return annotation.
    """
    provoke: Callable[[Any], None] | None = None
    """Do something that must raise `expect`, given the `ligase` module.

    For a failure that is not a disagreement between two inputs: a remote
    whose digest does not match is refused as the pipeline file is read,
    before there is a step for a diagnostic to be reported against.
    """


def fixture(name: str) -> str:
    """A fixture path, relative to the repository root.

    Relative to the root, not to the working directory: these strings end up
    in the diagnostics the site publishes, and `npm run build` runs this from
    web/ while a person runs it from the root. A path that depended on that
    would put a build machine's home directory on the published page.
    """
    path = FIXTURES / name
    if not path.is_file():
        raise SystemExit(f"fixture missing: {path}")
    return str(path.relative_to(ROOT))


class Served:
    """A fixture, served over a loopback socket, for the remote scenarios.

    Not a stub: the engine's own HTTP client fetches this over a real
    socket, so the diagnostics captured from it are the ones a user gets
    from a real archive. Loopback only, so the docs build reaches no
    network.
    """

    #: Ports to try, in order.
    #:
    #: Fixed rather than picked by the operating system, because the locator
    #: ends up inside a published diagnostic and `http://localhost:8731/...`
    #: reads as an example where `http://127.0.0.1:61401/...` reads as a
    #: machine's accident. Several, so a port already in use is not a failed
    #: docs build.
    PORTS = tuple(range(8731, 8741))

    def __init__(self, body: bytes) -> None:
        """Start on the first of `PORTS` that is free."""
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

        class Handler(BaseHTTPRequestHandler):
            # `do_GET` is the name `http.server` dispatches on.
            def do_GET(self) -> None:
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args: object) -> None:
                """Say nothing: a build log is not a web server log."""

        for port in self.PORTS:
            try:
                self._http = ThreadingHTTPServer(("localhost", port), Handler)
                break
            except OSError:
                continue
        else:
            raise SystemExit(
                "could not bind a local port for the remote diagnostics: "
                f"every one of {self.PORTS[0]}-{self.PORTS[-1]} is in use"
            )
        self._thread = threading.Thread(target=self._http.serve_forever, daemon=True)
        self._thread.start()

    @property
    def url(self) -> str:
        """Where the fixture is, as a locator.

        Written with `localhost` rather than the address it resolved to,
        because this string is published inside a diagnostic and is read as
        an example.
        """
        port = self._http.server_address[1]
        return f"http://localhost:{port}/HXB2.fasta"

    def stop(self) -> None:
        """Close the port."""
        self._http.shutdown()
        self._http.server_close()
        self._thread.join(timeout=5)


def a_remote_whose_digest_does_not_match(lg: Any) -> None:
    """Fetch a reference, pinned to a digest it does not have."""
    import tempfile

    served = Served((FIXTURES / "tiny_bare.fasta").read_bytes())
    try:
        with tempfile.TemporaryDirectory() as store:
            lg.remotes.configure(cache=store, offline=False)
            lg.remote(
                served.url,
                kind=lg.types.Reference,
                # Deliberately wrong, and obviously so: nothing hashes to
                # a run of zeroes.
                digest="0" * 64,
                organism="test",
            )
    finally:
        lg.remotes.reset()
        served.stop()


def a_locator_with_a_scheme_the_engine_does_not_have(lg: Any) -> None:
    """Name a remote with a fifth scheme."""
    lg.remote("ftp://ftp.example.org/HXB2.fasta", kind=lg.types.Reference)


def a_replay_with_no_record_of_the_query(lg: Any) -> None:
    """Replay a run whose query has never been recorded.

    No server is needed and none is started: the refusal happens before
    anything is fetched, which is the point of it. So the locator here is a
    plain example URL rather than a loopback one.
    """
    import tempfile

    try:
        with tempfile.TemporaryDirectory() as store:
            lg.remotes.configure(cache=store, offline=False, mode="replay")
            lg.query(
                "https://api.example.org/v4/targets?gene=EGFR",
                kind=lg.types.EntitySet,
                organism="Homo sapiens",
            )
    finally:
        lg.remotes.reset()


def a_stochastic_step_that_names_no_seed(lg: Any) -> None:
    """Declare a step stochastic and forget to say what seeds it.

    No pipeline is built and no file is read: the refusal happens as the
    decorator runs, which is the whole point of it. So this is a step
    definition and nothing else.

    The kind goes into this module's globals first; see the `TYPE_CHECKING`
    import at the top for why.
    """
    globals()["RankedList"] = lg.types.RankedList

    @lg.step(tool="iqtree", stochastic=True)  # type: ignore[untyped-decorator]
    def tree(ranking: RankedList, seed: int = 42) -> RankedList:
        raise AssertionError("the decorator refuses this step; the body never runs")


def a_seed_that_is_not_an_int(lg: Any) -> None:
    """Name a seed that is typed `str`."""
    globals()["Reads"] = lg.types.Reads

    @lg.step(tool="seqtk", stochastic=True, seed="seed")  # type: ignore[untyped-decorator]
    def sampled(reads: Reads, seed: str = "42") -> Reads:
        raise AssertionError("the decorator refuses this step; the body never runs")


def a_seed_without_declaring_the_step_stochastic(lg: Any) -> None:
    """Name a seed and leave `stochastic=` off."""
    globals()["Reads"] = lg.types.Reads

    @lg.step(tool="seqtk", seed="seed")  # type: ignore[untyped-decorator]
    def sampled(reads: Reads, seed: int = 42) -> Reads:
        raise AssertionError("the decorator refuses this step; the body never runs")


SCENARIOS: tuple[Scenario, ...] = (
    Scenario(
        key="wrongReferenceBuild",
        step="call",
        expect="DimensionError",
        description="Reads aligned to one build, variants called against another.",
        build=lambda lg: {
            "ref": lg.reference(
                fixture("tiny_bare.fasta"), organism="human", build="GRCh37"
            ),
            "aln": lg.alignment(
                fixture("tiny_other.sam"), organism="human", build="GRCh38"
            ),
        },
    ),
    Scenario(
        key="contigNaming",
        step="call",
        expect="ContigNamingError",
        description="`chr1` against `1`: the same genome under two naming conventions.",
        build=lambda lg: {
            "ref": lg.reference(fixture("tiny_bare.fasta"), organism="test"),
            "aln": lg.alignment(fixture("tiny_prefixed.sam"), organism="test"),
        },
    ),
    Scenario(
        key="shuffledSamples",
        step="merge",
        expect="DimensionError",
        description="The same samples in both files, in a different order.",
        build=lambda lg: {
            "left": lg.variants(fixture("tiny_two_samples.vcf"), organism="test"),
            "right": lg.variants(fixture("tiny_swapped_samples.vcf"), organism="test"),
        },
    ),
    Scenario(
        key="coordinateSystems",
        step="intersect",
        expect="DimensionError",
        description=(
            "BED's 0-based half-open intervals against GFF's 1-based closed ones."
        ),
        build=lambda lg: {
            "a": lg.intervals(fixture("tiny_bare.bed"), organism="test"),
            "b": lg.intervals(fixture("tiny_bare.gff"), organism="test"),
        },
    ),
    Scenario(
        key="referenceFrame",
        step="resistance",
        expect="ReferenceFrameError",
        pipeline=True,
        description=(
            "A pol-relative reference against gene windows stated in whole-genome "
            "K03455 coordinates."
        ),
        build=lambda lg: {
            "v": lg.variants(fixture("hivdb_pol.vcf"), organism="HIV-1"),
            "ref": lg.reference(fixture("hivdb_pol.fasta"), organism="HIV-1"),
        },
    ),
    Scenario(
        key="contigOrder",
        step="call",
        expect="ContigOrderError",
        description="Matching contigs in a different order -- which GATK rejects.",
        build=lambda lg: {
            "ref": lg.reference(fixture("tiny_reordered.fasta"), organism="test"),
            "aln": lg.alignment(fixture("tiny_bare.sam"), organism="test"),
        },
    ),
    Scenario(
        key="mixedIdentifiers",
        step="score",
        expect="DimensionError",
        description=("A gene list of HGNC symbols scored against one of Ensembl IDs."),
        build=lambda lg: {
            "truth": lg.entity_set(
                fixture("entities_hgnc.tsv"), organism="Homo sapiens"
            ),
            "found": lg.entity_set(
                fixture("entities_ensembl.tsv"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="rankedNamespace",
        step="score",
        expect="DimensionError",
        description=(
            "A ranked list of HGNC symbols scored against an Ensembl gene list."
        ),
        build=lambda lg: {
            "ranked": lg.ranked_list(
                fixture("ranked_degs.tsv"), organism="Homo sapiens"
            ),
            "truth": lg.entity_set(
                fixture("entities_ensembl.tsv"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="graphSources",
        step="compare",
        expect="DimensionError",
        description=(
            "Two graphs holding the same nodes and edges, one of which also "
            "folded in DisGeNET: the refusal `source_set` exists for."
        ),
        build=lambda lg: {
            "ours": lg.knowledge_graph(
                fixture("graph_targets.json"), organism="Homo sapiens"
            ),
            "theirs": lg.knowledge_graph(
                fixture("graph_targets_disgenet.json"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="graphCaseSensitiveSources",
        step="compare",
        expect="DimensionError",
        description=(
            "Two graphs whose sources differ only in case: two objects by "
            "specification, so two source sets (D36)."
        ),
        build=lambda lg: {
            "ours": lg.knowledge_graph(
                fixture("graph_source_upper.json"), organism="Homo sapiens"
            ),
            "theirs": lg.knowledge_graph(
                fixture("graph_source_lower.json"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="molecularStandardisation",
        step="overlap",
        expect="DimensionError",
        description=(
            "A library standardised and a hit list as the vendor shipped it: "
            "the same two substances, and an overlap of nothing."
        ),
        build=lambda lg: {
            "library": lg.molecule_set(
                fixture("molecules_library.tsv"), organism="Homo sapiens"
            ),
            "hits": lg.molecule_set(
                fixture("molecules_hits_salts.tsv"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="molecularTautomers",
        step="overlap",
        expect="DimensionError",
        description=(
            "Two libraries standardised to different states: the error names "
            "the operations one of them also applied."
        ),
        build=lambda lg: {
            "library": lg.molecule_set(
                fixture("molecules_library.tsv"), organism="Homo sapiens"
            ),
            "normalised": lg.molecule_set(
                fixture("molecules_tautomer_normalised.tsv"),
                organism="Homo sapiens",
            ),
        },
    ),
    Scenario(
        key="molecularUndeclared",
        step="overlap",
        expect="UndeclaredDimensionError",
        description=(
            "Two tables of InChIKeys, neither of which says what standardised "
            "them: demanded of, rather than agreeing by silence."
        ),
        build=lambda lg: {
            "library": lg.molecule_set(
                fixture("molecules_no_representation.tsv"), organism="Homo sapiens"
            ),
            "hits": lg.molecule_set(
                fixture("molecules_no_representation.tsv"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="graphNamespace",
        step="score",
        expect="DimensionError",
        description=(
            "A graph of Ensembl gene ids scored against a curated list of "
            "HGNC symbols: the mixed pair, demanded of."
        ),
        build=lambda lg: {
            "kg": lg.knowledge_graph(
                fixture("graph_targets.json"), organism="Homo sapiens"
            ),
            "truth": lg.entity_set(
                fixture("entities_hgnc.tsv"), organism="Homo sapiens"
            ),
        },
    ),
    Scenario(
        key="rankedRelease",
        step="score",
        expect="DimensionError",
        description=(
            "A ranking named by one HGNC release scored against a set named by another."
        ),
        build=lambda lg: {
            "ranked": lg.ranked_list(
                fixture("ranked_degs.tsv"),
                organism="Homo sapiens",
                release="HGNC 2026-09",
            ),
            "truth": lg.entity_set(
                fixture("entities_hgnc.tsv"),
                organism="Homo sapiens",
                release="HGNC 2026-03",
            ),
        },
    ),
    Scenario(
        key="remoteMoved",
        step="",
        expect="RemoteError",
        description=(
            "A remote pinned to a digest the source does not serve: the one "
            "failure a pin exists to produce."
        ),
        provoke=a_remote_whose_digest_does_not_match,
    ),
    Scenario(
        key="remoteScheme",
        step="",
        expect="RemoteError",
        description="A locator naming a scheme outside the closed set of four.",
        provoke=a_locator_with_a_scheme_the_engine_does_not_have,
    ),
    Scenario(
        key="replayWithoutRecord",
        step="",
        expect="RemoteError",
        description=(
            "A run that may only replay, asked for a query it has no record "
            "of: refused rather than quietly fetched."
        ),
        provoke=a_replay_with_no_record_of_the_query,
    ),
    Scenario(
        key="unseededStochasticStep",
        step="",
        expect="StepDefinitionError",
        description=(
            "A step that declares its tool stochastic without naming the "
            "parameter that seeds it: refused as the pipeline is read, not "
            "when it runs."
        ),
        provoke=a_stochastic_step_that_names_no_seed,
    ),
    Scenario(
        key="seedThatIsNotAnInt",
        step="",
        expect="StepDefinitionError",
        description=(
            "A seed named on a parameter that is not typed `int`: refused, "
            "because a seed a methods section cannot print is not one."
        ),
        provoke=a_seed_that_is_not_an_int,
    ),
    Scenario(
        key="seedWithoutDeclaring",
        step="",
        expect="StepDefinitionError",
        description=(
            "A seed named without declaring the step stochastic: refused "
            "rather than read as a shorthand for declaring it."
        ),
        provoke=a_seed_without_declaring_the_step_stochastic,
    ),
    Scenario(
        key="mixedOrganism",
        step="call",
        expect="DimensionError",
        description=(
            "A host-pathogen reference checked against a pathogen-only alignment."
        ),
        build=lambda lg: {
            "aln": lg.alignment(fixture("tiny_bare.sam"), organism="HIV-1"),
            "ref": lg.reference(
                fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"]
            ),
        },
    ),
)


def pipeline_errors(lg: Any, scenario: Scenario) -> list[Any]:
    """Check a one-step pipeline built from the scenario's inputs.

    A step's own coordinate frame comes from what it is annotated to
    return, so a reference in the wrong frame is only a disagreement once
    there is a step to disagree with. `CheckError.errors` carries every
    problem the check found, so the one-diagnostic rule below still bites.
    """
    from ligase import types

    def resistance(v: Any, ref: Any) -> Any:
        """Interpret variants as drug resistance."""
        return lg.hivdb.interpret(v, ref)

    # Set as objects rather than written as annotations: this module uses
    # `from __future__ import annotations`, so a written annotation is a
    # string resolved against the module's globals, and `ligase` is
    # imported inside a function here so that a missing extension gets the
    # explanation in `main` rather than an ImportError at import time.
    resistance.__annotations__ = {
        "v": types.Variants,
        "ref": types.Reference,
        "return": types.ResistanceReport,
    }

    assert scenario.build is not None
    pipe = lg.Pipeline(lg.step(tool="hivdb")(resistance)(**scenario.build(lg)))
    try:
        pipe.check()
    except lg.CheckError as error:
        return list(error.errors)
    return []


def provoked(lg: Any, scenario: Scenario) -> list[Any]:
    """Do the thing that must fail, and return what it raised.

    One error or none: the failures captured this way are refusals rather
    than reports, and a refusal is one exception.
    """
    assert scenario.provoke is not None
    try:
        scenario.provoke(lg)
    # Broad on purpose: the error *is* the output, and narrowing this would
    # mean this script deciding which classes a page may show.
    except Exception as error:
        return [error]
    return []


def capture(lg: Any, scenario: Scenario) -> str:
    """Run one scenario and return the engine's text, or exit with why not."""
    if scenario.provoke is not None:
        errors = provoked(lg, scenario)
    elif scenario.pipeline:
        errors = pipeline_errors(lg, scenario)
    else:
        assert scenario.build is not None
        errors = lg.dimension_errors(scenario.step, scenario.build(lg))

    if not errors:
        raise SystemExit(
            f"{scenario.key}: expected {scenario.expect}, got no error at all.\n"
            f"  The engine has stopped catching this. Either it regressed, or the "
            f"page describing it is now wrong."
        )
    if len(errors) != 1:
        kinds = ", ".join(type(e).__name__ for e in errors)
        raise SystemExit(
            f"{scenario.key}: expected exactly one {scenario.expect}, "
            f"got {len(errors)} "
            f"({kinds}). The page shows one diagnostic, so capturing several would "
            f"show the reader less than the engine said."
        )

    got = type(errors[0]).__name__
    if got != scenario.expect:
        raise SystemExit(
            f"{scenario.key}: expected {scenario.expect}, got {got}.\n"
            f"  A caller branching on the error type would now take a different "
            f"path, so this is a public API change, not a wording change."
        )

    text = str(errors[0]).rstrip("\n")
    if not text.strip():
        raise SystemExit(f"{scenario.key}: {got} stringified to nothing.")
    return text


def main() -> int:
    """Capture every scenario and write the module the site imports."""
    # Run from the repository root so that the fixture paths both open and
    # print the same way wherever this was invoked from -- `npm run build`
    # runs it from web/, a person runs it from the root, and the paths it
    # produces are published.
    os.chdir(ROOT)

    try:
        import ligase as lg
    except ImportError as exc:  # pragma: no cover - the CI failure mode
        print(
            f"error: cannot import ligase ({exc}).\n"
            "The docs build needs the compiled extension, because the diagnostics "
            "on the site are captured from a real run. Install it with "
            "`maturin develop` or `pip install -e .`.",
            file=sys.stderr,
        )
        return 1

    captured = {s.key: capture(lg, s) for s in SCENARIOS}
    described = {s.key: s.description for s in SCENARIOS}

    lines = [
        "// Generated by web/scripts/capture_diagnostics.py. Do not edit.",
        "//",
        "// Each string below is the Ligase engine's own output, captured from a",
        f"// real run against ligase {lg.version()}. The generator fails the build if",
        "// the engine stops producing the error a page claims it produces.",
        "",
        "export const diagnostics = {",
    ]
    for key, text in captured.items():
        lines.append(f"  /** {described[key]} */")
        lines.append(f"  {key}: {json.dumps(text)},")
    lines += [
        "} as const",
        "",
        "export type DiagnosticKey = keyof typeof diagnostics",
        "",
    ]

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(
        f"captured {len(captured)} diagnostics from ligase {lg.version()} -> {OUT.name}"
    )
    for key in captured:
        print(f"  {key}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
