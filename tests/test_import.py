"""M0 acceptance tests: the package installs, imports, and reports one version.

The version test guards a real failure mode: the Rust crate and the Python
package can drift apart, shipping a wheel whose reported version is a lie.
"""

from __future__ import annotations

import re
import tomllib
from pathlib import Path

# PEP 440 subset we intend to use: X.Y.Z with an optional pre/dev suffix.
PEP440 = re.compile(r"^\d+\.\d+\.\d+(?:(?:a|b|rc)\d+|\.dev\d+)?$")

REPO_ROOT = Path(__file__).resolve().parent.parent


def workspace_version() -> str:
    """The single source of truth for the project version."""
    manifest = REPO_ROOT / "Cargo.toml"
    with manifest.open("rb") as fh:
        data = tomllib.load(fh)
    version = data["workspace"]["package"]["version"]
    assert isinstance(version, str)
    return version


def test_import_ligase() -> None:
    import ligase

    assert ligase.__name__ == "ligase"


def test_version_is_pep440() -> None:
    import ligase

    assert PEP440.match(ligase.__version__), f"not PEP 440: {ligase.__version__!r}"


def test_version_matches_workspace_manifest() -> None:
    import ligase

    assert ligase.__version__ == workspace_version()


def test_core_version_comes_from_rust() -> None:
    """`__version__` must be reported by the compiled core, not hardcoded."""
    import ligase

    assert ligase._core.version() == ligase.__version__


def test_the_changelog_has_a_dated_section_for_this_version() -> None:
    """A bump with no changelog section behind it ships a silent release.

    v0.2.0 was tagged on a tree that still called itself 0.1.0, with every
    entry still under `[Unreleased]`. Had the wheels built, they would have
    been 0.1.0 wheels, PyPI would have skipped them as files it already had,
    and the run would have gone green having published nothing. The two
    halves of a release -- the number and what is written under it -- move
    together or not at all.
    """
    version = workspace_version()
    changelog = (REPO_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    dated = re.compile(
        rf"^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$", re.MULTILINE
    )

    assert dated.search(changelog), (
        f"CHANGELOG.md has no dated `## [{version}]` section. Move the"
        " `[Unreleased]` entries into one before releasing."
    )
