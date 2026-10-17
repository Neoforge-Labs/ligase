"""Ligase: type-safe, reproducible computational biology pipelines.

Biological data types carry dimensions (organism, reference build, coordinate
system, sample set), and the engine refuses to connect steps whose dimensions
do not match -- like units in physics.
"""

from __future__ import annotations

from . import _core

__all__ = ["__version__", "version"]

#: Version of the compiled core engine. Reported by Rust, never hardcoded.
__version__: str = _core.version()


def version() -> str:
    """Return the version of the compiled Ligase core engine."""
    return _core.version()
