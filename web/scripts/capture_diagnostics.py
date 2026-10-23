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
from typing import Any

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
    except Exception as error:  # noqa: BLE001 - the error is the output
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
