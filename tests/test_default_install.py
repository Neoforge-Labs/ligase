"""M17: what a default install is *not* allowed to need.

`pip install ligase-bio` is a genomics install. It must acquire nothing from
a chemistry toolkit, and the molecule kinds must still work: loading a
molecule table, reading its representation, hashing it and refusing to join
two that disagree are all the engine's work, and the engine has no chemistry
(D49).

That is a claim about imports, so it is checked against imports. A sentence
in the docs would say the same thing and would not notice the day somebody
moves `import rdkit` to the top of `chem.py` — which is exactly how an
install story is lost: not by a decision, by a convenience.

The Rust half of the same claim is `crates/ligase-core/tests/default_build.rs`.
"""

from __future__ import annotations

import subprocess
import sys

import pytest

import ligase as lg
from tests.conftest import fixture


def test_importing_ligase_imports_no_chemistry() -> None:
    # In a subprocess, because this suite imports RDKit elsewhere and a
    # check against this interpreter's `sys.modules` would pass for the
    # wrong reason.
    probe = (
        "import sys, ligase;"
        "print(any(name == 'rdkit' or name.startswith('rdkit.')"
        " for name in sys.modules))"
    )
    found = subprocess.run(
        [sys.executable, "-c", probe],
        capture_output=True,
        text=True,
        check=True,
    )
    assert found.stdout.strip() == "False", found.stdout


def test_a_molecule_set_loads_and_checks_with_no_toolkit_at_all(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # `sys.modules[name] = None` makes `import name` raise ImportError, so
    # this is the default install from inside a process that has RDKit.
    monkeypatch.setitem(sys.modules, "rdkit", None)

    library = lg.molecule_set(fixture("molecules_library.tsv"))
    read = library.dimension("representation")
    assert read is not None
    assert read.value == "InChIKey, salt-stripped and charge-neutralised"

    with pytest.raises(lg.DimensionError):
        lg.check_step(
            "overlap",
            {
                "library": library,
                "hits": lg.molecule_set(fixture("molecules_hits_salts.tsv")),
            },
        )


def test_standardising_without_the_toolkit_names_the_extra(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # The precedent is `ligase store status`, which tells a user which build
    # they have rather than leaving them to the release notes. A feature
    # that is absent should say so by name, and say what to type.
    monkeypatch.setitem(sys.modules, "rdkit", None)

    with pytest.raises(lg.StepDefinitionError) as refused:
        lg.chem.toolkit_version()

    said = str(refused.value)
    assert "pip install 'ligase-bio[chem]'" in said
    # And it says what *does* work without it, so a user reading this knows
    # whether they need it at all.
    assert "works on a default install" in said
