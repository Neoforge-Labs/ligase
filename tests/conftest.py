"""Shared paths and demo inputs for the Python tests.

The format fixtures live with the Rust tests rather than being duplicated
here. They are test *data* for the project, not for one language layer, and
two copies of a reference checksum is two chances to be wrong.

The demo's own data is not fixtures and not in the repository: since M10 the
demo declares where it comes from, and `ligase run` fetches it. So the gate
for a test that needs it is no longer "is this file on disk" but "can the
demo be built" — and building it is what checks every digest. Offline by
default, so running the suite never quietly downloads a hundred megabytes;
with the network where `LIGASE_DEMO_REQUIRED` says the machine was prepared
for the demo, which is CI.

Warm a store once with:

    python scripts/fetch_demo_data.py
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
from typing import TYPE_CHECKING, NoReturn

import pytest

import ligase as lg
import ligase.types

if TYPE_CHECKING:
    from collections.abc import Iterable
    from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / "crates" / "ligase-core" / "tests" / "fixtures"
#: Where derived demo artefacts go — the one BAM a real aligner wrote.
#: `LIGASE_DATA_DIR` moves it, matching `scripts/make_demo_alignment.sh`, so
#: the script and the tests cannot disagree about where it put things.
DATA = Path(os.environ.get("LIGASE_DATA_DIR", REPO_ROOT / "data"))

#: The demo pipeline, which is also the manifest of what it fetches.
DEMO = REPO_ROOT / "examples" / "hiv-resistance" / "pipeline.py"

#: Where the demo's fetched inputs are kept. The repository's own cache
#: directory unless told otherwise, so a developer and CI warm the same one.
DEMO_STORE = Path(os.environ.get("LIGASE_DEMO_STORE", REPO_ROOT / ".ligase"))


def unavailable(reason: str) -> NoReturn:
    """Skip a test for want of fetched data or of an installed tool.

    `data/` is gitignored and the demo's tools are not installed everywhere,
    so a test needing either skips rather than fails. Except on a machine
    that was prepared for the demo, which says so by setting
    `LIGASE_DEMO_REQUIRED`: there a skip is a bug in the preparation and not
    a fact about the machine, and reporting it as a skip would let CI pass a
    run in which the pipeline never ran.
    """
    if os.environ.get("LIGASE_DEMO_REQUIRED"):
        pytest.fail(f"LIGASE_DEMO_REQUIRED is set, but {reason}", pytrace=False)
    pytest.skip(reason)


def allowed_to_fetch() -> bool:
    """Whether a test may reach the network for the demo's inputs.

    Only where the machine says it was prepared for the demo. Everywhere
    else a cold store means a skip, because a test suite that downloads a
    hundred megabytes because you ran it is a test suite people stop
    running.
    """
    return bool(os.environ.get("LIGASE_DEMO_REQUIRED"))


def configure_demo_remotes() -> None:
    """Point this process's remotes at the demo's store.

    Called by every gate below rather than once, because `test_remotes.py`
    resets the setting between its own tests and the demo's gate must not
    depend on having run first.
    """
    lg.remotes.configure(cache=DEMO_STORE, offline=not allowed_to_fetch())


def demo_module() -> ModuleType:
    """The demo pipeline, loaded from its path.

    Importing it builds the pipeline, which resolves its remotes — so this
    raises `lg.RemoteError` where the inputs are neither in the store nor
    obtainable. That is the gate: the question a test wants answered is
    whether the demo can be built, and building it is what answers it.
    """
    specification = importlib.util.spec_from_file_location("ligase_demo", DEMO)
    assert specification is not None
    assert specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def require_demo(runs: Iterable[str] = (), project: str | None = None) -> ModuleType:
    """The demo module, or skip for want of its inputs.

    Args:
        runs: Runs to resolve beyond the ones the module resolves on import.
            `test_validation.py` needs the other BioProject's.
        project: Which project those runs belong to.

    Returns:
        The loaded demo module.
    """
    configure_demo_remotes()
    try:
        module = demo_module()
        for run in runs:
            module.run_reads(run, project or module.PROJECT)
    except lg.RemoteError as error:
        unavailable(f"the demo's inputs are not available: {error}")
    return module


def fixture(name: str) -> str:
    """Path to a committed format fixture."""
    path = FIXTURES / name
    assert path.is_file(), f"missing fixture: {path}"
    return str(path)


@pytest.fixture
def hxb2() -> str:
    """The real HXB2 reference, skipping if it is not available.

    A path into the content-addressed store rather than into `data/`: it is
    an input the engine fetched and checked, not a file somebody left
    lying about.
    """
    configure_demo_remotes()
    try:
        reference = demo_module().reference()
    except lg.RemoteError as error:
        unavailable(f"HXB2 is not available: {error}")
    assert isinstance(reference, lg.types.Reference)
    return reference.file


@pytest.fixture
def demo_bam() -> str:
    """The real HXB2 alignment, skipping if it has not been built."""
    alignments = sorted((DATA / "alignments").glob("*.bam"))
    if not alignments:
        unavailable(
            "data/alignments/*.bam is missing; run scripts/make_demo_alignment.sh"
        )
    return str(alignments[0])
