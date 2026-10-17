"""Shared paths for the Python tests.

The format fixtures live with the Rust tests rather than being duplicated
here. They are test *data* for the project, not for one language layer, and
two copies of a reference checksum is two chances to be wrong.
"""

from __future__ import annotations

from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / "crates" / "ligase-core" / "tests" / "fixtures"
DATA = REPO_ROOT / "data"


def fixture(name: str) -> str:
    """Path to a committed format fixture."""
    path = FIXTURES / name
    assert path.is_file(), f"missing fixture: {path}"
    return str(path)


@pytest.fixture
def hxb2() -> str:
    """The real HXB2 reference, skipping if it has not been fetched.

    `data/` is gitignored, so anything touching it must skip rather than fail.
    """
    path = DATA / "refs" / "HXB2.fasta"
    if not path.is_file():
        pytest.skip("run scripts/fetch_test_data.sh to fetch data/refs/HXB2.fasta")
    return str(path)


@pytest.fixture
def demo_bam() -> str:
    """The real HXB2 alignment, skipping if it has not been built."""
    alignments = sorted((DATA / "alignments").glob("*.bam"))
    if not alignments:
        pytest.skip("run scripts/make_demo_alignment.sh to build data/alignments/*.bam")
    return str(alignments[0])
