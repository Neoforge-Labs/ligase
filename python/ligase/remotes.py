"""How a run resolves its remote inputs.

A remote resolves when the pipeline declares it, not when a step runs: its
dimensions cannot be known until its bytes are in hand, and the engine's
whole claim is that dimensions are checked before any compute. That makes
resolution the *first* thing a run does, which is why it cannot be told where
to look through `pipeline.run(cache=...)` — by then the pipeline file has
already been read.

So the few settings a remote needs live here, as settings for the run rather
than for a step. `ligase run` sets them from its flags before it imports the
pipeline; a plain script gets the defaults.

    import ligase as lg
    from ligase.remotes import configure

    configure(cache="/scratch/ligase", mode="replay")
    hxb2 = lg.remote("ncbi:K03455.1", kind=lg.types.Reference, digest="...")
    genes = lg.query("https://api.example.org/targets?id=EGFR", kind=lg.types.EntitySet)

Nothing here changes what a pipeline *means*: the cache directory is where
bytes are kept, and a digest that matched in one directory matches in every
other.
"""

from __future__ import annotations

from pathlib import Path

from . import _core

__all__ = ["cache_directory", "configure", "mode", "offline", "reset", "seed"]

#: Where the cache goes when nothing says otherwise. The same default
#: `ligase.graph.CACHE_DIR` uses, kept in step by the import below.
_DEFAULT = ".ligase"

#: How a recorded query is treated, where nothing says otherwise. `auto`
#: replays a record that exists and records one that does not, which is what
#: a plain script wants.
_DEFAULT_MODE = "auto"

#: The three modes, as `configure` and `--replay`/`--refresh` name them.
MODES = ("auto", "replay", "refresh")

_cache: Path | None = None
_offline: bool = False
_mode: str = _DEFAULT_MODE


def configure(
    *,
    cache: Path | str | None = None,
    offline: bool | None = None,
    mode: str | None = None,
) -> None:
    """Set how this run resolves its remotes.

    Call it before the pipeline is built, because that is when remotes
    resolve. `ligase run` does so from its own flags; a script calls it
    directly, or not at all.

    Args:
        cache: Where to keep the store a remote is fetched into. The
            default is `./.ligase`, which is where everything else a run
            caches goes. `None` leaves the current setting alone, so a
            caller can set one thing without knowing about the others.
        offline: Whether this run may fetch anything. With `True`, a remote
            the store already holds with a matching digest resolves as
            usual and anything else is an error -- which is what a reviewer
            with no connection wants, and what makes "the second run needs
            no network" a thing a test can prove rather than infer. `None`
            leaves the current setting alone.
        mode: How a recorded query is treated: `"auto"` replays a record
            that exists and records one that does not, `"replay"` refuses a
            query it has no record of, and `"refresh"` fetches anyway and
            reports what changed. A mode is per run rather than per step,
            because a run that replayed some of its queries and fetched the
            rest would be reporting a result nobody can repeat. `None`
            leaves the current setting alone.

    Raises:
        ValueError: If `mode` is not one of the three.
    """
    # A per-run setting is per process: there is one engine, one cache
    # directory, and one import of the pipeline file.
    global _cache, _offline, _mode
    if cache is not None:
        _cache = Path(cache)
    if offline is not None:
        _offline = offline
    if mode is not None:
        if mode not in MODES:
            known = ", ".join(MODES)
            message = f"`{mode}` is not a mode. The modes are {known}"
            raise ValueError(message)
        _mode = mode


def cache_directory() -> Path:
    """Where a remote is fetched into.

    Returns:
        The directory `configure` was given, or `./.ligase`.
    """
    return _cache if _cache is not None else Path(_DEFAULT)


def offline() -> bool:
    """Whether this run may fetch anything.

    Returns:
        `True` if only the store may answer.
    """
    return _offline


def mode() -> str:
    """How this run treats a recorded query.

    Returns:
        `"auto"`, `"replay"` or `"refresh"`.
    """
    return _mode


def seed(bundle: Path | str, *, cache: Path | str | None = None) -> int:
    """Put a bundle's recorded responses into the cache, so the run replays.

    This is the precise sense in which a bundle no longer has to be able to
    fetch its own data: for a recorded query the bundle **is** the source.
    A reviewer on a clean machine with no network and no API key seeds a
    cache from the bundle and the pipeline replays from it. A pinned remote
    is unaffected, because it is named rather than carried -- anyone can
    obtain it and check it against its digest.

    Call it before the pipeline is built, for the same reason `configure` has
    to be called then: a query resolves while the pipeline file is being
    read. `ligase methods --verify` does so itself.

    Args:
        bundle: The bundle directory, as `ligase methods --bundle` wrote it.
            A bundle with no recorded queries adds nothing and is not an
            error.
        cache: Which store to seed. The default is whatever `configure`
            was told, or `./.ligase`.

    Returns:
        How many records were added. A record the cache already holds is
            left as it is, so calling this twice adds nothing the second
            time.

    Raises:
        RemoteError: If the bundle cannot be read or the cache cannot be
            written.
    """
    where = Path(cache) if cache is not None else cache_directory()
    return int(_core.seed_records(str(bundle), str(where)))


def reset() -> None:
    """Forget what `configure` was told.

    For tests, and for a process that runs two pipelines with different
    settings. A setting that leaked from one run into the next would be a
    run whose inputs came from somewhere its own report does not name.
    """
    global _cache, _offline, _mode
    _cache = None
    _offline = False
    _mode = _DEFAULT_MODE
