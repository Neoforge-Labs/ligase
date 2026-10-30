# Contributing to Ligase

Thank you for considering it. This file says how the project works, so that
a change you spend time on is a change that can be merged.

Ligase is a Rust core (`crates/ligase-core`) with Python bindings
(`crates/ligase-py`, PyO3 + maturin) and a Python surface
(`python/ligase`). Its design argument is in
[CLAUDE.md](CLAUDE.md), and every design decision is written up with its
reasoning at [the decisions
page](https://ligase.neoforgelabs.tech/project/decisions/). If you are
proposing something that changes behaviour, read the decision that covers it
first — it may already have been considered and refused for a stated reason,
and the fastest way to change the answer is to argue against that reason.

## The scope rule

A new biological *kind* earns its place in Ligase only if it brings a
**dimension** whose mismatch is a documented silent failure in real research.
A kind that is a file format with a schema is a parser, and Ligase does not
need more parsers. This is
[D35](https://ligase.neoforgelabs.tech/project/decisions/), and the answer to
a kind that fails the test is no, however convenient the kind would be for
one project.

This is the most common reason a well-written pull request is declined, so it
is worth checking before you write one.

## Getting set up

You need a stable Rust toolchain, a C compiler, and Python 3.11 or newer.

```bash
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"      # builds the extension via maturin
```

After changing Rust code, rebuild the bindings:

```bash
maturin develop
```

## Running what CI runs

Both suites must pass. Run them before opening a pull request, because a
green local run is the only thing that makes a review about your change
rather than about the build.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all

ruff check .
ruff format --check .
mypy
pytest
```

CI runs six jobs, and all of them gate a merge:

| Job | What it checks |
|---|---|
| `rust` | `cargo fmt`, `cargo clippy -D warnings`, `cargo test` |
| `python` | ruff, `mypy --strict`, pytest on Python 3.11 and 3.14, Linux and macOS |
| `chem` | the RDKit extra, where the chemistry tests **must run, not skip** |
| `demo` | the HIV-1 reference pipeline over real reads with the real tools |
| `store` | the S3 remote store against a real S3 server |
| `shell` | shellcheck over `scripts/*.sh` |

The documentation build (`.github/workflows/web.yml`) is a seventh gate, and
it is stricter than it looks — see below.

Some tests skip when what they need is absent: the chemistry tests without
RDKit, the store tests without `LIGASE_TEST_STORE`, the demo without `bwa`,
`samtools` and `bcftools`. That is deliberate, and so is the fact that the
`chem` and `demo` jobs **fail on a skip**: a test that silently checks
something else is worse than one that says it did not run.

### Do not weaken a gate to make something pass

If a check fails, the check is the thing that was right. Do not loosen a
clippy lint, mark a test `xfail`, widen a tolerance or relax a CI guard in
order to go green. If you believe a gate is genuinely wrong, say so in the
pull request and leave it failing; that is a conversation, not a blocker.

## Tests

- Every public function gets tests.
- **Every bug fix gets a regression test** that fails before the fix.
- Test names are sentences about behaviour, not `test_foo_2`. Look at
  `tests/` and `crates/ligase-core/tests/` for the house style.

A number is only as good as the run behind it. If your change alters what the
engine reports, add or update the scenario that captures it rather than
editing the expected text by hand — see the next section.

## Documentation is part of the change

A feature is not done until its documentation is written, in the same pull
request:

- `web/content/concepts/<feature>.mdx` — what it is and why it exists
- `web/content/guide/<feature>.mdx` — how to use it, with a runnable example
- `web/content/project/architecture/<component>.mdx` — how it works inside,
  and the trade-offs
- the page added to the `_meta.ts` in its directory, or Nextra will not show
  it
- an entry in [CHANGELOG.md](CHANGELOG.md)

**Engine output on a page comes from a real run, never from imagination.**
Several files are generated on every docs build and are gitignored — the API
reference, the CLI reference, the captured diagnostics, the quickstart
transcript, the notebooks. Never paste engine output into a page: add a
scenario to the relevant `web/scripts/capture_*.py`, and the build will fail
if the engine stops producing what the page claims.

The gate is:

```bash
cd web && npm run check
```

It regenerates, builds, and then fails on a broken link, a dangling anchor, a
page nothing links to, or a Markdown link that never resolved.

Claims about use or adoption must be true. The project has no outside users;
please do not imply otherwise, and do not add quotes, logos or metrics that
do not exist.

## Commits and pull requests

- Small, focused commits with [conventional
  commit](https://www.conventionalcommits.org/) messages (`feat:`, `fix:`,
  `docs:`, `chore:`, `refactor:`, `test:`).
- Branch names are `<type>/<slug>`, using the same types.
- Say in the pull request what you ran and what it printed. "Tests pass" is
  not evidence; the output is.
- No AI attribution anywhere — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footers, no bot markers — in commit messages,
  pull request descriptions, code comments or documentation.

## Ask first about

- **Adding a dependency.** Both ecosystems. Say what it does and what it
  would cost to do without it.
- **Changing a public API.** If a change would refuse a pipeline that used to
  pass, it needs an entry in [MIGRATING.md](MIGRATING.md) saying what to do.
- **Adding a new kind or dimension.** See the scope rule above.

Open an issue before writing the code. It is a cheaper conversation than a
closed pull request.

## Licensing of contributions

Ligase is dual-licensed under either the [MIT](LICENSE-MIT) or the [Apache
License 2.0](LICENSE-APACHE), at the user's option. Unless you state
otherwise, any contribution you intentionally submit for inclusion is
licensed the same way, with no additional terms.

## Reporting a problem, and getting help

See [SUPPORT.md](SUPPORT.md). Conduct expectations are in
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
