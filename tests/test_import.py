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
