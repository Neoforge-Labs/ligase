"""M10: `lg.remote`, against a server on this machine.

The fetch that starts nearly every pipeline used to sit outside everything
Ligase promises. These tests are about it being inside: a remote is an input
like any other, with dimensions read from its own bytes, and a digest that
does not match is a failure before any step runs.

The server is `http.server` from the standard library, on a loopback port.
That is a scheme Ligase accepts for exactly this reason — TLS protects a
channel against something sitting in it, and between two ends of a loopback
socket there is nothing to sit in.
"""

from __future__ import annotations

import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import TYPE_CHECKING

import pytest

import ligase as lg
import ligase.types
from ligase.types import Reference

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

#: A one-contig FASTA whose name is an accession, as `K03455.1` is.
HXB2 = b">K03455.1 Human immunodeficiency virus 1\nACGTACGTAC\nGTACGTACGT\n"

#: The same sequence, reflowed and the defline reworded. What NCBI does from
#: time to time, and what must not break a pin.
HXB2_REFLOWED = b">K03455.1 HIV-1 isolate HXB2\nACGTACGTACGTACGTACGT\n"


class Server:
    """A local HTTP server that serves what it is told to, and counts asks."""

    def __init__(self) -> None:
        """Start on a port the operating system picks."""
        self.bodies: dict[str, bytes] = {}
        self.statuses: dict[str, int] = {}
        self.hits: dict[str, int] = {}
        self.headers: list[dict[str, str]] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            # `do_GET` is the name `http.server` dispatches on.
            def do_GET(self) -> None:
                outer.hits[self.path] = outer.hits.get(self.path, 0) + 1
                outer.headers.append(dict(self.headers.items()))
                body = outer.bodies.get(self.path)
                status = outer.statuses.get(self.path, 200 if body else 404)
                self.send_response(status)
                self.send_header("Content-Length", str(len(body or b"")))
                self.end_headers()
                if body:
                    self.wfile.write(body)

            def log_message(self, *_args: object) -> None:
                """Say nothing: pytest's output is not a web server log."""

        self._http = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._thread = threading.Thread(target=self._http.serve_forever, daemon=True)
        self._thread.start()

    @property
    def base(self) -> str:
        """The server's base URL."""
        host, port = self._http.server_address[:2]
        if isinstance(host, bytes):  # pragma: no cover - platform dependent
            host = host.decode()
        return f"http://{host}:{port}"

    def url(self, path: str) -> str:
        """A URL on this server."""
        return f"{self.base}{path}"

    def serve(self, path: str, body: bytes, status: int = 200) -> None:
        """Answer this path with these bytes."""
        self.bodies[path] = body
        self.statuses[path] = status

    def stop(self) -> None:
        """Close the port, so a fetch to it fails to connect."""
        self._http.shutdown()
        self._http.server_close()
        self._thread.join(timeout=5)


@pytest.fixture
def server() -> Iterator[Server]:
    """A local HTTP server, stopped when the test ends."""
    running = Server()
    try:
        yield running
    finally:
        running.stop()


@pytest.fixture(autouse=True)
def _cache(tmp_path: Path) -> Iterator[None]:
    """A store of its own for each test, and no setting left behind.

    `offline` is set explicitly rather than left alone. `configure` leaves
    an argument it was not given, which is right for a caller setting one
    thing — and means a test running after one that configured the demo's
    store offline would otherwise inherit it and fail to fetch. These tests
    run in a random order, so that is a certainty rather than a risk.
    """
    lg.remotes.configure(cache=tmp_path / ".ligase", offline=False)
    try:
        yield
    finally:
        lg.remotes.reset()


def fetch(locator: str, digest: str | None = None) -> Reference:
    """Resolve a locator as a reference, as the demo does."""
    held = lg.remote(
        locator,
        kind=Reference,
        digest=digest,
        organism="HIV-1",
        build="HXB2",
    )
    assert isinstance(held, Reference)
    return held


# ------------------------------------------------------------ the basics --


def test_a_remote_resolves_and_carries_the_dimensions_of_its_kind(
    server: Server,
) -> None:
    server.serve("/HXB2.fasta", HXB2)

    hxb2 = fetch(server.url("/HXB2.fasta"))

    # Read by the same loader a local FASTA goes through, so the contigs and
    # their checksums are inferred rather than taken on trust.
    assert hxb2.kind == "Reference"
    contigs = hxb2.contigs
    assert contigs is not None
    assert [contig.name for contig in contigs] == ["K03455.1"]
    assert contigs[0].length == 20
    organism = hxb2.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1"


def test_a_second_run_needs_no_network(server: Server) -> None:
    server.serve("/HXB2.fasta", HXB2)
    locator = server.url("/HXB2.fasta")
    first = fetch(locator)
    digest = _digest_of(first)

    server.stop()
    again = fetch(locator, digest=digest)

    # The bytes and their digest are in the store, so resolution is a
    # lookup. This is what makes a pipeline built on remotes run offline
    # once it has run online.
    assert again.contigs == first.contigs
    assert server.hits[("/HXB2.fasta")] == 1


def test_the_digest_of_what_arrived_is_reported_so_it_can_be_pinned(
    server: Server,
) -> None:
    server.serve("/HXB2.fasta", HXB2)

    hxb2 = fetch(server.url("/HXB2.fasta"))

    # Nothing was promised on the first run, so the engine says what it got
    # and the user pastes it in. A digest you cannot obtain is a digest
    # nobody pins.
    digest = _digest_of(hxb2)
    assert len(digest) == 64
    assert all(character in "0123456789abcdef" for character in digest)


# ------------------------------------------------------- a wrong digest --


def test_a_wrong_digest_raises_before_any_step_runs(server: Server) -> None:
    server.serve("/HXB2.fasta", HXB2)
    wrong = "0" * 64

    with pytest.raises(lg.RemoteError) as raised:
        fetch(server.url("/HXB2.fasta"), digest=wrong)

    message = str(raised.value)
    assert "moved" in message
    assert wrong in message
    assert server.url("/HXB2.fasta") in message


def test_a_wrong_digest_fails_while_the_pipeline_is_still_being_read(
    server: Server,
) -> None:
    server.serve("/HXB2.fasta", HXB2)

    # The point of resolving at declaration time: there is no graph yet, no
    # step has been decorated, and nothing could have run. A design that
    # deferred the fetch would have to arrange this; this one cannot avoid
    # it.
    with pytest.raises(lg.RemoteError):
        fetch(server.url("/HXB2.fasta"), digest="1" * 64)


def test_a_reflow_that_changes_no_biology_does_not_break_the_pin(
    server: Server,
) -> None:
    server.serve("/HXB2.fasta", HXB2)
    locator = server.url("/HXB2.fasta")
    pinned = _digest_of(fetch(locator))

    # A different store, as a machine that has never seen this remote has.
    lg.remotes.reset()
    server.serve("/HXB2.fasta", HXB2_REFLOWED)
    again = fetch(locator, digest=pinned)

    # The sequence is the same sequence. A pin that broke here would teach
    # you to bump pins without reading, and then the one that matters gets
    # bumped too.
    assert _digest_of(again) == pinned


def test_a_change_in_the_sequence_itself_is_a_failure(server: Server) -> None:
    server.serve("/HXB2.fasta", HXB2)
    locator = server.url("/HXB2.fasta")
    pinned = _digest_of(fetch(locator))

    lg.remotes.reset()
    server.serve("/HXB2.fasta", b">K03455.1 HIV-1\nTTTTTTTTTTTTTTTTTTTT\n")

    with pytest.raises(lg.RemoteError, match="moved"):
        fetch(locator, digest=pinned)


# ---------------------------------------------------------- the locator --


def test_an_unknown_scheme_names_the_four() -> None:
    with pytest.raises(lg.RemoteError) as raised:
        fetch("ftp://ftp.example.org/x.fasta")

    message = str(raised.value)
    for scheme in ("sra:", "ncbi:", "ensembl:", "https:"):
        assert scheme in message


def test_a_bare_accession_is_refused_rather_than_guessed() -> None:
    # `SRR32217726` looks like an SRA run and `K03455.1` like an NCBI
    # accession, so a guess would be right often enough to be trusted and
    # then fetch from the wrong archive.
    with pytest.raises(lg.RemoteError, match="sra:"):
        fetch("K03455.1")


def test_an_unknown_kind_says_which_kinds_there_are() -> None:
    # A class of the right shape that is not one of the engine's kinds. The
    # annotation says `type[Dataset]`, so this is what the mistake looks
    # like when it gets past the type checker.
    class Genome(lg.types.Dataset):
        """Not a kind Ligase knows."""

    with pytest.raises(ValueError, match="kinds are"):
        lg.remote("https://example.org/x", kind=Genome)


def test_a_digest_that_is_not_a_digest_says_what_one_looks_like() -> None:
    with pytest.raises(ValueError, match="hexadecimal"):
        lg.remote("https://example.org/x", kind=Reference, digest="deadbeef")


# ------------------------------------------------------ what 200 can be --


def test_an_error_page_served_as_two_hundred_is_refused(server: Server) -> None:
    # NCBI answers a bad accession with a 200 and an HTML page. Nothing in
    # the status says so, and with no digest declared there is nothing to
    # compare against -- so reading the file is the only thing between that
    # page and a pipeline treating it as a reference.
    server.serve("/bad.fasta", b"<html><body>Error: ID list is empty</body></html>")

    with pytest.raises(lg.RemoteError) as raised:
        fetch(server.url("/bad.fasta"))

    assert "cannot be read as Reference" in str(raised.value)


def test_a_source_that_answers_with_a_failure_says_so(server: Server) -> None:
    server.serve("/missing.fasta", b"", status=404)

    with pytest.raises(lg.RemoteError, match="404"):
        fetch(server.url("/missing.fasta"))


def test_a_source_that_cannot_be_reached_says_so(server: Server) -> None:
    locator = server.url("/x.fasta")
    server.stop()

    with pytest.raises(lg.RemoteError) as raised:
        fetch(locator)

    assert locator in str(raised.value)


# ------------------------------------------------------------ no secrets --


def test_nothing_the_engine_sends_or_keeps_holds_a_secret(server: Server) -> None:
    server.serve("/HXB2.fasta", HXB2)

    hxb2 = fetch(server.url("/HXB2.fasta"))

    # The engine sends no authorization header of its own, and has nowhere
    # to record one: `Fetched` has no field for request headers, so the
    # guarantee is structural rather than a filter a later field could
    # quietly undo.
    sent = server.headers[-1]
    assert not any(name.lower() == "authorization" for name in sent)
    assert "authorization" not in repr(hxb2).lower()


def _digest_of(dataset: Reference) -> str:
    """The digest the engine recorded for this remote.

    Read back off the loaded dataset rather than recomputed, because the
    point of the test is what the *engine* says it got.
    """
    origin = dataset.origin
    assert origin is not None
    return origin.digest
