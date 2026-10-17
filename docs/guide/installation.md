# Installation

## Requirements

| | |
|---|---|
| Python | 3.11 or newer |
| Rust | a stable toolchain ([rustup](https://rustup.rs)) |
| curl | for fetching test data |

Rust is needed because installing builds the engine from source. There are no
pre-built wheels yet.

## Install

```bash
git clone git@github.com:Neoforge-Labs/ligase.git
cd ligase
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
```

The first build takes a minute or two while `pyo3` compiles. Later builds are
incremental.

Check it worked:

```bash
python -c "import ligase; print(ligase.__version__)"
```

```text
0.1.0
```

!!! note "The distribution and the import have different names"
    The PyPI distribution is **`ligase-bio`** but you `import ligase`. So it
    is `pip install ligase-bio` and `import ligase`. The distribution name
    `ligase` is already taken on PyPI by an unrelated project, and the import
    name is the one you actually live with, so that is the one that stayed
    short.

## Run the checks

The same gate CI runs, in the order it runs them:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
ruff check .
ruff format --check .
mypy
pytest
```

All seven should pass on a clean checkout. If `clippy` complains about a
missing doc comment on something you just added, that is deliberate — the
workspace sets `missing_docs` to warn and CI runs clippy with `-D warnings`,
so public items without documentation fail the build.

### Rebuilding after a Rust change

An editable install does **not** recompile on import. After editing anything
under `crates/`, rebuild:

```bash
maturin develop
```

Or `pip install -e .` again, which is slower but equivalent. Forgetting this
is the single most common confusion in a mixed Rust/Python project: your
Python changes take effect immediately and your Rust changes do not.

## Test data

The reference pipeline runs on real HIV-1 data. It is downloaded rather than
committed, into a gitignored `data/`:

```bash
scripts/fetch_test_data.sh
```

```text
==> Reference: HXB2 (K03455.1)
  downloading from NCBI
  verified: 9719 bp, canonical SHA-256 matches

==> Reads: PRJNA1218827
  SRR32217714_1.fastq.gz verified
  ...
  4 runs ready in data/reads/PRJNA1218827
```

You get:

| Path | What |
|---|---|
| `data/refs/HXB2.fasta` | HIV-1 HXB2 reference, NCBI `K03455.1`, 9719 bp |
| `data/reads/PRJNA517147/` | 4 paired runs, ~100 MB — published resistance results |
| `data/reads/PRJNA1218827/` | 4 paired runs, ~30 MB — Sanger ground truth |

Useful flags:

```bash
scripts/fetch_test_data.sh --list                     # show the manifest
scripts/fetch_test_data.sh --project PRJNA1218827     # just the small set
scripts/fetch_test_data.sh --reference-only           # just HXB2
scripts/fetch_test_data.sh --force                    # re-download
```

Set `LIGASE_DATA_DIR` to put the data somewhere else — useful if you keep
large files on another volume.

### What the script guarantees

Every file is verified against a checksum pinned in the script, and the script
exits non-zero if one does not match. Re-running is safe and cheap: files
already on disk that verify are not downloaded again, so this doubles as an
integrity check on data you fetched weeks ago.

Reads are pinned by ENA's own per-file MD5. The reference is pinned by the
SHA-256 of its **canonical sequence** — sequence only, uppercased, line breaks
removed — rather than of the raw file. This is
[decision D4](../decisions.md) applied to an input: NCBI can reflow a FASTA
or reword a defline without the biology changing, and a cosmetic change
upstream must not break your build. A real change to the sequence still fails,
loudly.

FASTQs come from ENA over HTTPS, so no SRA toolkit is required — only `curl`.

## Build the docs

```bash
pip install -e ".[docs]"
mkdocs serve
```

Then open <http://127.0.0.1:8000>. CI builds with `mkdocs build --strict`, so
a broken internal link fails the build rather than shipping.

## Troubleshooting

**`No module named 'ligase'` after a successful install.**
You are probably in a different interpreter than the one you installed into.
Check with `python -c "import sys; print(sys.executable)"`.

**Rust changes have no effect.**
Run `maturin develop`. See [above](#rebuilding-after-a-rust-change).

**`error: linker 'cc' not found`.**
No C toolchain. On Debian or Ubuntu, `apt install build-essential`; on macOS,
`xcode-select --install`.

**`mkdocs build --strict` fails on a link you just added.**
Internal links are relative to the file they live in, and must include the
`.md` extension — `../decisions.md`, not `/decisions`.
