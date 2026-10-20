"""The vendored resistance algorithms, and the files derived from them.

Two files are committed for each algorithm: Stanford's ASI2 XML exactly as
it is published, and the `.ruleset` the engine reads, derived from it by
`scripts/vendor_hivdb_algorithm.py`.

Committing a derived file is only safe if something re-derives it. These
tests do, on every run: they check the XML against the SHA-256 pinned in
`PROVENANCE.tsv`, re-derive the `.ruleset` from it, and compare. A ruleset
edited by hand, or an XML swapped for another version, fails here rather
than quietly changing what a mutation means.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
from types import ModuleType

from tests.conftest import REPO_ROOT

#: The vendoring script, loaded by path: `scripts/` is not a package.
SCRIPT = REPO_ROOT / "scripts" / "vendor_hivdb_algorithm.py"

#: Where the algorithms and their pins live.
RULESETS = REPO_ROOT / "crates" / "ligase-core" / "rulesets"


def _script() -> ModuleType:
    """The vendoring script, as a module."""
    specification = importlib.util.spec_from_file_location("ligase_vendor", SCRIPT)
    assert specification is not None
    assert specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def test_every_derived_ruleset_still_matches_its_source() -> None:
    """A `.ruleset` says what its XML says, or the build fails."""
    finished = subprocess.run(
        [sys.executable, str(SCRIPT), "--check"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert finished.returncode == 0, finished.stderr or finished.stdout


def test_the_pinned_algorithms_are_the_ones_we_think() -> None:
    """The pins name the versions the validation depends on."""
    pins = {pin["file"]: pin for pin in _script()._pins()}

    # 9.8 is what the published results in `tests/test_validation.py` were
    # produced with, so it is the version those comparisons are made
    # against. 10.2 is the current algorithm, shipped so that a user is
    # not stuck with a 2025 interpretation of their 2026 sample.
    assert pins["HIVDB_9.8.xml"]["version"] == "9.8"
    assert pins["HIVDB_9.8.xml"]["date"] == "2025-01-05"
    assert pins["HIVDB_10.2.xml"]["version"] == "10.2"

    for pin in pins.values():
        assert (RULESETS / pin["file"]).is_file()
        assert (RULESETS / pin["file"].replace(".xml", ".ruleset")).is_file()
        assert len(pin["sha256"]) == 64
        assert pin["source"].startswith("https://github.com/hivdb/hivfacts/")


def test_the_licence_is_recorded_where_someone_will_look() -> None:
    """Vendored third-party data says where it came from and on what terms.

    Not a style check. The algorithm is redistributed here, and a
    redistribution whose licence lives only in a commit message is one
    nobody downstream can check.
    """
    readme = (RULESETS / "README.md").read_text()

    assert "Unlicense" in readme
    assert "public domain" in readme
    assert "hivdb/hivfacts" in readme
    # The citation Stanford asks for.
    assert "Rhee SY" in readme
    assert "Nucleic Acids Research" in readme
    # GPL-3.0 code that must stay out of an MIT/Apache project.
    assert "hivdb/sierra" in readme
