"""Shared paths for the Python tests.

The format fixtures live with the Rust tests rather than being duplicated
here. They are test *data* for the project, not for one language layer, and
two copies of a reference checksum is two chances to be wrong.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import NoReturn

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / "crates" / "ligase-core" / "tests" / "fixtures"
DATA = REPO_ROOT / "data"


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


def fixture(name: str) -> str:
    """Path to a committed format fixture."""
    path = FIXTURES / name
    assert path.is_file(), f"missing fixture: {path}"
    return str(path)


@pytest.fixture
def hxb2() -> str:
    """The real HXB2 reference, skipping if it has not been fetched."""
    path = DATA / "refs" / "HXB2.fasta"
    if not path.is_file():
        unavailable("data/refs/HXB2.fasta is missing; run scripts/fetch_test_data.sh")
    return str(path)


@pytest.fixture
def demo_bam() -> str:
    """The real HXB2 alignment, skipping if it has not been built."""
    alignments = sorted((DATA / "alignments").glob("*.bam"))
    if not alignments:
        unavailable(
            "data/alignments/*.bam is missing; run scripts/make_demo_alignment.sh"
        )
    return str(alignments[0])
