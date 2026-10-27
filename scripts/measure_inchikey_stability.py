"""Measure InChIKey stability under every standardisation state, one RDKit.

The measurement §4.4 of the v0.2 design asked for before committing to
InChIKey as a canonical form. It writes one JSON document to stdout for
whatever RDKit is installed; run it once per version and compare the
documents.

To repeat the measurement M17 was decided on — six releases over four years,
each in its own throwaway environment, 1,080 keys in all:

    for v in 2022.9.5 2023.9.6 2024.9.6 2025.9.6 2026.3.6 2026.9.1; do
      uv run --python 3.11 --no-project --with "rdkit==$v" \
        python scripts/measure_inchikey_stability.py > "out-$v.json"
    done
    python - <<'EOF'
    import glob, json
    docs = [json.load(open(p)) for p in sorted(glob.glob("out-*.json"))]
    for state in docs[0]["results"]:
        for name in docs[0]["results"][state]:
            keys = {d["results"][state][name].get("inchikey") for d in docs}
            if len(keys) > 1:
                print("differs:", state, name, keys)
    EOF

It found none. The values the design quotes are asserted on one interpreter
by `tests/test_inchikey_stability.py`, which is the part a test suite can
check; this script is the part that needs several interpreters.
"""

import json
import sys
from typing import Any

import rdkit
from rdkit import Chem, RDLogger
from rdkit.Chem import inchi
from rdkit.Chem.MolStandardize import rdMolStandardize

RDLogger.DisableLog("rdApp.*")

MOLECULES = {
    "aspirin": "CC(=O)Oc1ccccc1C(=O)O",
    "aspirin-sodium-salt": "CC(=O)Oc1ccccc1C(=O)[O-].[Na+]",
    "ibuprofen-S": "C[C@@H](c1ccc(CC(C)C)cc1)C(=O)O",
    "ibuprofen-R": "C[C@H](c1ccc(CC(C)C)cc1)C(=O)O",
    "ibuprofen-flat": "CC(c1ccc(CC(C)C)cc1)C(=O)O",
    "warfarin-keto": "CC(=O)CC(c1ccccc1)c1c(O)c2ccccc2oc1=O",
    "warfarin-enol": "CC(O)=CC(c1ccccc1)c1c(O)c2ccccc2oc1=O",
    "guanine-keto": "Nc1nc2[nH]cnc2c(=O)[nH]1",
    "guanine-enol": "Nc1nc(O)c2nc[nH]c2n1",
    "2-pyridone": "O=c1cccc[nH]1",
    "2-hydroxypyridine": "Oc1ccccn1",
    "imatinib": "Cc1ccc(NC(=O)c2ccc(CN3CCN(C)CC3)cc2)cc1Nc1nccc(-c2cccnc2)n1",
    "imatinib-mesylate": (
        "CS(=O)(=O)O.Cc1ccc(NC(=O)c2ccc(CN3CCN(C)CC3)cc2)cc1Nc1nccc(-c2cccnc2)n1"
    ),
    "metformin-hcl": "CN(C)C(=N)NC(N)=N.Cl",
    "metformin": "CN(C)C(=N)NC(N)=N",
    "glycine-neutral": "NCC(=O)O",
    "glycine-zwitterion": "[NH3+]CC(=O)[O-]",
    "nitrobenzene-charged": "O=[N+]([O-])c1ccccc1",
    "nitrobenzene-pentavalent": "O=N(=O)c1ccccc1",
    "caffeine": "Cn1c(=O)c2c(ncn2C)n(C)c1=O",
    "carbazole": "c1ccc2c(c1)[nH]c1ccccc12",
    "phosphoric-acid": "OP(=O)(O)O",
    "phosphate-trianion": "[O-]P(=O)([O-])[O-]",
    "tetracycline": (
        "C[C@]1(O)[C@@H]2C[C@H]3[C@@H](N(C)C)C(=O)C(C(N)=O)=C(O)[C@@]3(O)"
        "C(=O)C2=C(O)c2c(O)cccc21"
    ),
    "cisplatin": "N.N.Cl[Pt]Cl",
    "vancomycin-fragment": "CN[C@H](CC(C)C)C(=O)N[C@@H]1[C@H](O)c2ccc(O)cc2",
    "histidine": "N[C@@H](Cc1c[nH]cn1)C(=O)O",
    "oseltamivir": "CCOC(=O)C1=C[C@@H](OC(CC)CC)[C@H](NC(C)=O)[C@@H](N)C1",
    "sodium-valproate": "CCCC(CCC)C(=O)[O-].[Na+]",
    "choline-chloride": "C[N+](C)(C)CCO.[Cl-]",
}


def strip_salt(mol: Any) -> Any:
    """Remove counterions and solvents, leaving the parent."""
    return rdMolStandardize.FragmentParent(mol)


def normalise_tautomer(mol: Any) -> Any:
    """Choose one tautomer for every tautomeric form."""
    return rdMolStandardize.TautomerEnumerator().Canonicalize(mol)


def neutralise(mol: Any) -> Any:
    """Remove formal charges where a neutral form exists."""
    return rdMolStandardize.Uncharger().uncharge(mol)


def cleanup(mol: Any) -> Any:
    """RDKit's own sanitisation, which every state is measured after."""
    return rdMolStandardize.Cleanup(mol)


STATES = {
    "raw": [],
    "cleaned": [cleanup],
    "salt-stripped": [cleanup, strip_salt],
    "tautomer-normalised": [cleanup, normalise_tautomer],
    "charge-neutralised": [cleanup, neutralise],
    "all-three": [cleanup, strip_salt, neutralise, normalise_tautomer],
}


def key_for(smiles: str, steps: list[Any]) -> dict[str, str]:
    """The InChIKey of `smiles` after `steps`, or why there is none."""
    mol = Chem.MolFromSmiles(smiles)
    if mol is None:
        return {"error": "unparsed"}
    try:
        for step in steps:
            mol = step(mol)
            if mol is None:
                return {"error": "standardisation returned None"}
        return {
            "inchikey": inchi.MolToInchiKey(mol),
            "smiles": Chem.MolToSmiles(mol),
        }
    except Exception as exc:
        return {"error": f"{type(exc).__name__}: {exc}"}


def main() -> None:
    """Write one JSON document for this interpreter's RDKit."""
    out = {
        "rdkit_version": rdkit.__version__,
        "python": sys.version.split()[0],
        "inchi_version": getattr(inchi, "INCHI_VERSION", None)
        or getattr(inchi, "_inchiVersion", None),
        "results": {
            state: {name: key_for(smiles, steps) for name, smiles in MOLECULES.items()}
            for state, steps in STATES.items()
        },
    }
    json.dump(out, sys.stdout, indent=1, sort_keys=True)


main()
