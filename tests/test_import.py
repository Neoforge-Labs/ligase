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


def _section(document: str, heading: str) -> str:
    """One `## <heading>` section of a Markdown document, without its title.

    Up to the next `## `, so a section's own `###` subsections are part of
    it. Empty when there is no such heading.
    """
    pattern = re.compile(
        rf"^## {re.escape(heading)}\s*$(.*?)(?=^## |\Z)", re.MULTILINE | re.DOTALL
    )
    found = pattern.search(document)
    return found.group(1) if found else ""


def test_a_changelog_entry_that_points_at_migrating_has_a_section_there() -> None:
    """D57: a result-altering change is named where a pin is chosen.

    A changelog body is not a release signal. A consumer pinned
    `ligase-bio>=0.1.0` picks a minor bump up on their next lock, and two
    integrations lost a debugging cycle to migrations documented only
    inside a long entry. So a change that alters results or refuses
    something that used to pass links to `MIGRATING.md`, and this is what
    makes the pair hold: a link with nothing behind it fails here rather
    than shipping a pointer into an empty page.

    Checked for `[Unreleased]` and for the released version, because those
    are the two sections a release moves between.
    """
    changelog = (REPO_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    migrating = (REPO_ROOT / "MIGRATING.md").read_text(encoding="utf-8")
    version = workspace_version()

    for heading, wanted in (("[Unreleased]", "Unreleased"), (f"[{version}]", version)):
        entries = _section(changelog, heading)
        if not entries or "MIGRATING.md" not in entries:
            continue
        assert _section(migrating, wanted), (
            f"CHANGELOG.md's `{heading}` section points at MIGRATING.md, which"
            f" has no `## {wanted}` section. Write what breaks and the edit"
            " that fixes it: a pointer into an empty page is worse than no"
            " pointer, because a consumer who follows it stops looking."
        )


def test_every_migrating_section_is_a_version_the_changelog_knows() -> None:
    """The pair holds in both directions.

    A migration note under a version the changelog never mentions is a note
    nobody reaches: `MIGRATING.md` is read *from* a release, so a heading
    with no release behind it is unreachable by the route it exists for.
    """
    changelog = (REPO_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    migrating = (REPO_ROOT / "MIGRATING.md").read_text(encoding="utf-8")

    for heading in re.findall(r"^## (.+?)\s*$", migrating, re.MULTILINE):
        if heading == "Unreleased":
            assert "## [Unreleased]" in changelog
            continue
        # A heading may cover several releases that needed the same edit,
        # as the `frame=` migration shipped in two.
        for version in re.findall(r"\d+\.\d+\.\d+", heading):
            assert f"## [{version}]" in changelog, (
                f"MIGRATING.md has a `## {heading}` section and CHANGELOG.md"
                f" has no `## [{version}]`. One of the two is wrong about what"
                " was released."
            )
