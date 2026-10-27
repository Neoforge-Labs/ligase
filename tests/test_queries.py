"""M10b: recording a live query, and replaying it.

A `RecordedQuery` promises the same bytes forever *from the record*, because
its source promises nothing. The mechanism is one thing — the canonicalised
response body goes into the content-addressed store rather than merely its
digest — and everything these tests check follows from it: a run completes
with the server down, a reviewer replays a bundle on a clean machine, and a
token the request carried is nowhere in either.

Everything here runs against a real socket on localhost, so "the server
down" is a closed port rather than a stubbed error.
"""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

import ligase as lg
from ligase.types import EntitySet, KnowledgeGraph
from tests.conftest import fixture

if TYPE_CHECKING:
    from collections.abc import Iterator

#: A gene list, as a TSV query endpoint answers with one.
GENES = b"id\tscore\nTP53\t0.91\nBRCA1\t0.72\n"

#: The same endpoint after the source changed its mind, which is what a live
#: source is entitled to do.
GENES_CHANGED = b"id\tscore\nTP53\t0.91\nBRCA1\t0.55\n"


class Server:
    """A local HTTP server that serves what it is told to, and counts asks.

    Its own rather than `test_remotes.py`'s, because a query test needs two
    things that one does not: a body that can be changed between runs, so a
    refresh has drift to find, and request headers kept per path, so "no
    trace of a token" can be checked against what was actually sent.
    """

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
        """Answer this path with these bytes, replacing any earlier answer."""
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

    Every field is set explicitly rather than left alone: `configure` keeps
    an argument it was not given, which is right for a caller setting one
    thing and means a test running after one that set `replay` would
    otherwise inherit it. These tests run in a random order, so that is a
    certainty rather than a risk.
    """
    lg.remotes.configure(cache=tmp_path / ".ligase", offline=False, mode="auto")
    try:
        yield
    finally:
        lg.remotes.reset()


def ask(locator: str, **declared: object) -> EntitySet:
    """Resolve a query as a gene list."""
    held = lg.query(
        locator,
        kind=EntitySet,
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
        **declared,  # type: ignore[arg-type]
    )
    assert isinstance(held, EntitySet)
    return held


def digest_of(dataset: EntitySet) -> str:
    """The digest the engine recorded for this query."""
    origin = dataset.origin
    assert origin is not None
    return origin.digest


# ------------------------------------------------------- record and replay --


def test_a_query_records_its_answer_and_replays_it_without_asking_again(
    server: Server,
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")

    first = ask(url)
    second = ask(url)

    assert digest_of(first) == digest_of(second)
    assert server.hits["/genes"] == 1


def test_the_run_that_records_is_live_and_the_next_is_replayed(
    server: Server,
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")

    first = ask(url)
    second = ask(url)

    first_origin, second_origin = first.origin, second.origin
    assert first_origin is not None
    assert second_origin is not None
    assert first_origin.verdict == "live"
    assert second_origin.verdict == "replayed"


def test_a_recorded_pipeline_runs_again_with_the_server_down_to_an_identical_result(
    server: Server, tmp_path: Path
) -> None:
    """The headline clause, and the whole reason records exist."""
    server.serve("/genes", GENES)
    url = server.url("/genes")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    first = lg.Pipeline(rank(ask(url))).run(cache=store)
    [output] = first.outputs("rank")
    produced = Path(output).read_bytes()

    # The source goes away entirely. Anything that still works from here on
    # worked out of the record.
    server.stop()

    second = lg.Pipeline(rank(ask(url))).run(cache=store)
    [again] = second.outputs("rank")

    assert Path(again).read_bytes() == produced
    assert again == output


def test_replay_on_a_query_with_no_record_fails_before_any_step_runs(
    server: Server,
) -> None:
    server.serve("/genes", GENES)
    lg.remotes.configure(mode="replay")

    with pytest.raises(lg.RemoteError) as raised:
        # There is no graph yet when this fails: a query resolves while the
        # pipeline file is still being read, which is what makes "before any
        # step runs" true rather than merely intended.
        ask(server.url("/genes"))

    message = str(raised.value)
    assert "no record" in message
    assert "--replay" in message
    # Never a silent live fetch.
    assert server.hits.get("/genes", 0) == 0


def test_two_paginated_calls_recorded_in_one_order_replay_in_the_other(
    server: Server,
) -> None:
    server.serve("/genes?page=1", b"id\tscore\nTP53\t0.91\n")
    server.serve("/genes?page=2", b"id\tscore\nBRCA1\t0.72\n")
    one, two = server.url("/genes?page=1"), server.url("/genes?page=2")

    first, second = ask(one), ask(two)

    # Recorded in one order, replayed in the other, and each still its own
    # answer: a record is keyed by what was asked, not by when.
    lg.remotes.configure(mode="replay")
    assert digest_of(ask(two)) == digest_of(second)
    assert digest_of(ask(one)) == digest_of(first)
    assert digest_of(first) != digest_of(second)


def test_a_query_asked_with_its_parameters_reordered_replays_one_record(
    server: Server,
) -> None:
    server.serve("/genes?a=1&b=2", GENES)

    ask(server.url("/genes?a=1&b=2"))

    lg.remotes.configure(mode="replay")
    assert digest_of(ask(server.url("/genes?b=2&a=1"))) is not None


# ------------------------------------------------------------------ refresh --


def test_a_refresh_reports_drift_without_overwriting_the_record(
    server: Server, tmp_path: Path
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")
    store = tmp_path / "store"
    lg.remotes.configure(cache=store)

    first = ask(url)

    # The source changes its mind, which a live source is entitled to do.
    server.serve("/genes", GENES_CHANGED)
    lg.remotes.configure(mode="refresh")
    refreshed = ask(url)

    assert digest_of(refreshed) != digest_of(first)

    # The earlier record is still there and still replayable, so the run
    # that produced the earlier result can still be reproduced.
    lg.remotes.configure(mode="replay")
    assert digest_of(ask(url)) == digest_of(refreshed)
    records = sorted((store / "store").rglob("*"))
    assert len([held for held in records if held.is_file()]) >= 2


def test_a_refresh_names_both_bodies_in_the_run_report(
    server: Server, tmp_path: Path
) -> None:
    # "Reports the drift" means a user sees it, not that the engine computed
    # it: the note travels with the dataset into the run report.
    server.serve("/genes", GENES)
    url = server.url("/genes")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    lg.Pipeline(rank(ask(url))).run(cache=store)

    server.serve("/genes", GENES_CHANGED)
    lg.remotes.configure(mode="refresh")
    refreshed = lg.Pipeline(rank(ask(url))).run(cache=store)

    assert "has changed since it was recorded" in refreshed.text
    # Both bodies named, so the two records can be compared rather than
    # merely disagreed about.
    assert "Both records are kept" in refreshed.text


def test_a_refresh_that_finds_the_same_answer_reports_no_drift(
    server: Server, tmp_path: Path
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    lg.Pipeline(rank(ask(url))).run(cache=store)

    lg.remotes.configure(mode="refresh")
    quiet = lg.Pipeline(rank(ask(url))).run(cache=store)

    # A quiet quarter costs nothing and says nothing.
    assert "has changed since it was recorded" not in quiet.text


def test_a_refresh_with_no_record_is_a_first_record_rather_than_drift(
    server: Server,
) -> None:
    server.serve("/genes", GENES)
    lg.remotes.configure(mode="refresh")

    # Nothing to compare against is not a change, so this is an ordinary
    # success rather than a report of a changed query.
    assert digest_of(ask(server.url("/genes"))) is not None


def test_the_run_report_says_where_each_input_came_from(
    server: Server, tmp_path: Path
) -> None:
    # "It worked" and "it worked without asking anybody" are different
    # statements, and the report has to say which this was.
    server.serve("/genes", GENES)
    url = server.url("/genes")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    first = lg.Pipeline(rank(ask(url))).run(cache=store)
    assert "answered live on" in first.text

    second = lg.Pipeline(rank(ask(url))).run(cache=store)
    assert "replayed from record of" in second.text


def test_replay_and_refresh_together_are_refused() -> None:
    # One says never ask the source and the other says always ask it.
    from ligase.cli import build_parser

    with pytest.raises(SystemExit):
        build_parser().parse_args(["run", "--replay", "--refresh"])


def test_a_mode_that_is_not_a_mode_says_what_the_modes_are() -> None:
    with pytest.raises(ValueError, match="auto, replay, refresh"):
        lg.remotes.configure(mode="rewind")


# ----------------------------------------------------------------- the bundle --


def test_a_bundle_replays_on_a_clean_machine_with_no_network(
    server: Server, tmp_path: Path
) -> None:
    """A reviewer's situation: a bundle, a fresh store, and nothing else."""
    server.serve("/genes", GENES)
    url = server.url("/genes")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    recorded_store = tmp_path / "recorded"
    lg.remotes.configure(cache=recorded_store)
    bundle = tmp_path / "bundle"
    written = lg.Pipeline(rank(ask(url))).methods(cache=recorded_store, bundle=bundle)
    expected = digest_of(ask(url))

    assert written.bundle is not None
    assert (bundle / "records.tsv").is_file()
    assert any((bundle / "records").iterdir())

    # A clean machine: a store that has never seen this query, and a source
    # that is gone.
    server.stop()
    clean = tmp_path / "clean"
    lg.remotes.configure(cache=clean, mode="replay")

    added = lg.remotes.seed(bundle, cache=clean)
    assert added == 1

    replayed = ask(url)
    assert digest_of(replayed) == expected

    report = lg.Pipeline(rank(replayed)).run(cache=clean)
    assert [step.outcome for step in report.steps] == ["ran"]


def test_seeding_a_bundle_twice_adds_nothing_the_second_time(
    server: Server, tmp_path: Path
) -> None:
    server.serve("/genes", GENES)
    recorded_store = tmp_path / "recorded"
    lg.remotes.configure(cache=recorded_store)
    bundle = tmp_path / "bundle"

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    lg.Pipeline(rank(ask(server.url("/genes")))).methods(
        cache=recorded_store, bundle=bundle
    )

    clean = tmp_path / "clean"
    assert lg.remotes.seed(bundle, cache=clean) == 1
    assert lg.remotes.seed(bundle, cache=clean) == 0


def test_a_bundle_from_a_run_with_no_query_carries_no_records_table(
    tmp_path: Path,
) -> None:
    # A bundle should not carry an empty table explaining a feature the run
    # did not use.
    @lg.step
    def align(reference: lg.Reference) -> lg.Alignment:
        """Pretend to align, so the run has a step."""
        return lg.sh("cp {reference} {out}")

    bundle = tmp_path / "bundle"
    pipe = lg.Pipeline(
        align(lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny"))
    )
    pipe.methods(cache=tmp_path / "store", bundle=bundle)

    assert not (bundle / "records.tsv").exists()


def test_the_methods_paragraph_says_the_responses_are_in_the_bundle(
    server: Server, tmp_path: Path
) -> None:
    server.serve("/genes", GENES)

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    lg.remotes.configure(cache=store)
    written = lg.Pipeline(rank(ask(server.url("/genes")))).methods(cache=store)

    paragraph = written.text
    assert "Responses were recorded from" in paragraph
    # The honest claim, and the one a pinned remote does not need to make.
    assert "included in the reproducibility bundle" in paragraph


# ------------------------------------------------------- no trace of a token --


def test_an_auth_header_leaves_no_trace_in_the_record_or_the_bundle(
    server: Server, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    token = "sk-live-do-not-record-me"
    monkeypatch.setenv("LIGASE_TEST_TOKEN", token)
    server.serve("/private", GENES)

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    store = tmp_path / "store"
    lg.remotes.configure(cache=store)
    genes = ask(server.url("/private"), auth_env="LIGASE_TEST_TOKEN")

    # It really was sent. A test that proved nothing was stored without
    # proving anything was sent would pass against a request that never
    # carried a token at all.
    sent = {name.lower(): value for name, value in server.headers[-1].items()}
    assert sent.get("authorization") == f"Bearer {token}"

    bundle = tmp_path / "bundle"
    lg.Pipeline(rank(genes)).methods(cache=store, bundle=bundle)

    # Asserted over the whole store and the whole bundle rather than over a
    # list of header names, so a field added later cannot reintroduce the
    # leak quietly.
    for root in (store, bundle):
        for held in root.rglob("*"):
            if not held.is_file():
                continue
            blob = held.read_bytes()
            assert token.encode() not in blob, f"{held} holds the token"
            assert b"Authorization" not in blob, f"{held} holds the header"


def test_an_auth_env_that_is_not_set_says_which_variable_it_wanted(
    server: Server,
) -> None:
    server.serve("/private", GENES)

    with pytest.raises(lg.RemoteError, match="LIGASE_TEST_ABSENT"):
        ask(server.url("/private"), auth_env="LIGASE_TEST_ABSENT")

    # Never sent unauthenticated: a 401 would be the lucky outcome, and a
    # smaller public subset of the data recorded as the answer the unlucky
    # one.
    assert server.hits.get("/private", 0) == 0


def test_an_authenticated_run_and_a_bare_one_replay_the_same_record(
    server: Server, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("LIGASE_TEST_TOKEN", "sk-live-secret")
    server.serve("/private", GENES)
    url = server.url("/private")

    recorded_with_token = ask(url, auth_env="LIGASE_TEST_TOKEN")

    # The reviewer has the bundle and no token. A token is how you are let
    # in, not what you asked for, so it is no part of which record this is.
    lg.remotes.configure(mode="replay")
    assert digest_of(ask(url)) == digest_of(recorded_with_token)


# ------------------------------------------------------------ what is refused --


def test_an_answer_too_big_to_record_says_to_pin_it_as_a_release(
    server: Server,
) -> None:
    # 32 MiB and one byte. A body this size is a dataset, and a dataset has
    # a version to pin.
    server.serve("/huge", b"x" * (32 * 1024 * 1024 + 1))

    with pytest.raises(lg.RemoteError, match="pin this as a release instead"):
        ask(server.url("/huge"))


def test_a_source_that_answers_with_a_failure_records_nothing(
    server: Server,
) -> None:
    server.serve("/genes", b"", status=500)
    url = server.url("/genes")

    with pytest.raises(lg.RemoteError):
        ask(url)

    # A record of a failure would replay the failure forever as though it
    # were an answer.
    lg.remotes.configure(mode="replay")
    with pytest.raises(lg.RemoteError, match="no record"):
        ask(url)


def test_an_unknown_kind_says_which_kinds_there_are(server: Server) -> None:
    server.serve("/genes", GENES)

    class Genome(lg.types.Dataset):
        """A kind of the right shape that Ligase does not have."""

    with pytest.raises(ValueError, match="not a kind of data Ligase knows"):
        lg.query(server.url("/genes"), kind=Genome)


# ------------------------------------------------------------ thin bundles --


def test_a_thin_bundle_carries_no_bodies_and_names_the_store(
    server: Server, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """M14: what `--thin` leaves out, and what it says about it."""
    server.serve("/genes", GENES)
    url = server.url("/genes")
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    monkeypatch.setenv("LIGASE_STORE", f"file://{bucket}")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    recorded_store = tmp_path / "recorded"
    lg.remotes.configure(cache=recorded_store)
    bundle = tmp_path / "bundle"
    written = lg.Pipeline(rank(ask(url))).methods(
        cache=recorded_store, bundle=bundle, thin=True
    )

    assert written.bundle is not None
    assert (bundle / "records.tsv").is_file(), "the record is still named"
    assert not (bundle / "records").exists(), "its body is not carried"

    store = (bundle / "store.tsv").read_text()
    assert str(bucket) in store, store

    # And the README says what has stopped being promised, because the
    # paragraph above it has just promised the opposite.
    readme = (bundle / "README.md").read_text()
    assert "thin bundle" in readme
    assert "no network and no API key" in readme
    assert "does not make that promise" in readme


def test_a_thin_bundle_replays_where_the_store_can_be_reached(
    server: Server, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    monkeypatch.setenv("LIGASE_STORE", f"file://{bucket}")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    recorded_store = tmp_path / "recorded"
    lg.remotes.configure(cache=recorded_store)
    bundle = tmp_path / "bundle"
    lg.Pipeline(rank(ask(url))).methods(cache=recorded_store, bundle=bundle, thin=True)
    expected = digest_of(ask(url))

    # A collaborator: the source is gone, the bundle has no bodies, and the
    # store is reachable. That is the case `--thin` exists for.
    server.stop()
    clean = tmp_path / "clean"
    lg.remotes.configure(cache=clean, mode="replay")

    assert lg.remotes.seed(bundle, cache=clean) == 1
    assert digest_of(ask(url)) == expected


def test_a_thin_bundle_without_the_store_fails_before_any_step_runs(
    server: Server, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    server.serve("/genes", GENES)
    url = server.url("/genes")
    bucket = tmp_path / "bucket"
    bucket.mkdir()
    monkeypatch.setenv("LIGASE_STORE", f"file://{bucket}")

    @lg.step
    def rank(genes: lg.EntitySet) -> lg.RankedList:
        """Turn a scored gene list into a ranking."""
        return lg.sh("cp {genes} {out}")

    recorded_store = tmp_path / "recorded"
    lg.remotes.configure(cache=recorded_store)
    bundle = tmp_path / "bundle"
    lg.Pipeline(rank(ask(url))).methods(cache=recorded_store, bundle=bundle, thin=True)

    # The store is now out of reach, which for a thin bundle is fatal and
    # has to say so rather than replaying some queries and not others.
    server.stop()
    monkeypatch.setenv("LIGASE_STORE", f"file://{tmp_path / 'nowhere'}")
    clean = tmp_path / "clean"
    lg.remotes.configure(cache=clean, mode="replay")

    with pytest.raises(lg.RemoteError) as raised:
        lg.remotes.seed(bundle, cache=clean)

    message = str(raised.value)
    assert "thin bundle" in message, message
    assert str(bucket) in message, "it names the store it was told about"
    assert "--thin" in message, "and how to get a bundle that does not need it"


# ------------------------------------------------ the JSON branch, for real --


#: A knowledge graph, as a JSON query endpoint answers with one.
#:
#: The server's own bookkeeping is in it on purpose: `generatedAt` is exactly
#: the field `response.py`'s closed set exists for, and a graph fetched twice
#: a minute apart has to be one graph.
GRAPH = json.dumps(
    {
        "generatedAt": "2026-10-27T09:14:02Z",
        "nodes": [
            {"id": "ENSG00000141510", "type": "gene", "sources": ["open-targets"]},
            {"id": "ENSG00000012048", "type": "gene", "sources": ["open-targets"]},
        ],
        "edges": [
            {
                "source": "ENSG00000141510",
                "target": "ENSG00000012048",
                "type": "interacts_with",
                "sources": ["open-targets"],
            }
        ],
    }
).encode()

#: The same endpoint after one more source was folded in upstream, which is
#: a different graph however little else changed.
GRAPH_WITH_DISGENET = json.dumps(
    {
        "nodes": [
            {"id": "ENSG00000141510", "type": "gene", "sources": ["open-targets"]},
            {"id": "ENSG00000012048", "type": "gene", "sources": ["open-targets"]},
        ],
        "edges": [
            {
                "source": "ENSG00000141510",
                "target": "ENSG00000012048",
                "type": "interacts_with",
                "sources": ["open-targets", "disgenet"],
            }
        ],
    }
).encode()


def ask_for_a_graph(locator: str, **declared: object) -> KnowledgeGraph:
    """Resolve a query as a knowledge graph."""
    held = lg.query(
        locator,
        kind=KnowledgeGraph,
        organism="Homo sapiens",
        **declared,  # type: ignore[arg-type]
    )
    assert isinstance(held, KnowledgeGraph)
    return held


def test_a_recorded_graph_replays_with_the_server_down(
    server: Server, tmp_path: Path
) -> None:
    """M15 closes the gap M10b left named.

    `response.py` canonicalises a JSON body — keys sorted at every depth, the
    server's own fields dropped from the top level — and until there was a
    kind that reads JSON, that branch was exercised by unit tests and never
    end to end. A `KnowledgeGraph` is that kind, so this is the first test in
    which a JSON body goes through the recorder, into the store, and out
    again through a loader that parses it as what it claims to be.
    """
    server.serve("/graph", GRAPH)
    url = server.url("/graph")

    @lg.step
    def prune(kg: lg.KnowledgeGraph) -> lg.KnowledgeGraph:
        """Drop the edges below a score, which this test does not care about."""
        return lg.sh("cp {kg} {out}")

    store = tmp_path / "store"
    first = lg.Pipeline(prune(ask_for_a_graph(url))).run(cache=store)
    [output] = first.outputs("prune")
    produced = Path(output).read_bytes()

    # The source goes away entirely. Anything that still works worked out of
    # the record.
    server.stop()

    second = lg.Pipeline(prune(ask_for_a_graph(url))).run(cache=store)
    [again] = second.outputs("prune")

    assert Path(again).read_bytes() == produced
    assert again == output


def test_a_queried_graphs_sources_are_read_out_of_the_answer(
    server: Server,
) -> None:
    # The dimension comes off the body the service returned, so a graph
    # fetched from a service carries the same axis a graph on disk does --
    # and the loader is the same loader, which is what stops the two
    # disagreeing.
    server.serve("/graph", GRAPH)

    held = ask_for_a_graph(server.url("/graph"))

    found = held.dimension("source_set")
    assert found is not None
    assert found.value == "open-targets"


def test_a_graph_that_gained_a_source_upstream_is_a_different_answer(
    server: Server, tmp_path: Path
) -> None:
    # `--refresh` finds the drift, and the drift is the source composition
    # rather than anything about the biology. A pipeline that re-asked on
    # every run would have taken the new graph for the old one.
    server.serve("/graph", GRAPH)
    url = server.url("/graph")
    lg.remotes.configure(cache=tmp_path / "store")

    first = ask_for_a_graph(url)
    assert first.origin is not None

    server.serve("/graph", GRAPH_WITH_DISGENET)
    lg.remotes.configure(mode="refresh")
    refreshed = ask_for_a_graph(url)
    assert refreshed.origin is not None

    assert refreshed.origin.digest != first.origin.digest
    found = refreshed.dimension("source_set")
    assert found is not None
    assert found.value == "open-targets and disgenet"


def test_a_servers_signature_does_not_make_a_graph_a_new_graph(
    server: Server, tmp_path: Path
) -> None:
    # The JSON branch's whole point, reached end to end for the first time:
    # the same graph served a minute later, with a different `generatedAt`,
    # is the same graph. Without the drop every run would see drift and
    # everything below the query would rerun.
    served = json.loads(GRAPH)
    server.serve("/graph", json.dumps(served).encode())
    url = server.url("/graph")
    lg.remotes.configure(cache=tmp_path / "store")

    first = ask_for_a_graph(url)
    assert first.origin is not None

    served["generatedAt"] = "2026-10-27T11:52:40Z"
    server.serve("/graph", json.dumps(served).encode())
    lg.remotes.configure(mode="refresh")
    again = ask_for_a_graph(url)
    assert again.origin is not None

    assert again.origin.digest == first.origin.digest
    # And the engine says which field it dropped, in case it was data.
    assert any("generatedAt" in note for note in again.notes), again.notes
