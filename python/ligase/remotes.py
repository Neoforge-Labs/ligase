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

    configure(cache="/scratch/ligase")
    hxb2 = lg.remote("ncbi:K03455.1", kind=lg.types.Reference, digest="...")

Nothing here changes what a pipeline *means*: the cache directory is where
bytes are kept, and a digest that matched in one directory matches in every
other.
"""

from __future__ import annotations

from pathlib import Path

__all__ = ["cache_directory", "configure", "reset"]

#: Where the cache goes when nothing says otherwise. The same default
#: `ligase.graph.CACHE_DIR` uses, kept in step by the import below.
_DEFAULT = ".ligase"

_cache: Path | None = None


def configure(*, cache: Path | str | None = None) -> None:
    """Set how this run resolves its remotes.

    Call it before the pipeline is built, because that is when remotes
    resolve. `ligase run` does so from its own flags; a script calls it
    directly, or not at all.

    Args:
        cache: Where to keep the store a remote is fetched into. The
            default is `./.ligase`, which is where everything else a run
            caches goes. `None` leaves the current setting alone, so a
            caller can set one thing without knowing about the others.
    """
    # A per-run setting is per process: there is one engine, one cache
    # directory, and one import of the pipeline file.
    global _cache
    if cache is not None:
        _cache = Path(cache)


def cache_directory() -> Path:
    """Where a remote is fetched into.

    Returns:
        The directory `configure` was given, or `./.ligase`.
    """
    return _cache if _cache is not None else Path(_DEFAULT)


def reset() -> None:
    """Forget what `configure` was told.

    For tests, and for a process that runs two pipelines with different
    settings. A setting that leaked from one run into the next would be a
    run whose inputs came from somewhere its own report does not name.
    """
    global _cache
    _cache = None
