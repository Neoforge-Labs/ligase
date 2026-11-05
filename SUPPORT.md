# Getting help with Ligase

## Read first

Most questions have a page, and the documentation is organised by what you
are trying to do rather than by how the project is built:

| | |
|---|---|
| [Quickstart](https://ligase.neoforgelabs.tech/start/quickstart/) | five minutes, no aligner, no downloads |
| [Silent failures](https://ligase.neoforgelabs.tech/silent-failures/) | the bugs it catches, **titled by symptom** |
| [Guides](https://ligase.neoforgelabs.tech/guide/building-a-pipeline/) | one task at a time |
| [Concepts](https://ligase.neoforgelabs.tech/concepts/types-and-dimensions/) | why it works this way |
| [Errors](https://ligase.neoforgelabs.tech/reference/errors/) | every diagnostic, with what to do about it |
| [Dimensions](https://ligase.neoforgelabs.tech/reference/dimensions/) | which formats state which dimension |
| [Decisions](https://ligase.neoforgelabs.tech/project/decisions/) | every design choice, with its argument |

If Ligase refused your pipeline and you want to know why, start with
[Errors](https://ligase.neoforgelabs.tech/reference/errors/) — the
diagnostic's own name is the heading. If something produced a wrong number
and you are trying to find out how, the
[silent failures](https://ligase.neoforgelabs.tech/silent-failures/) pages
are titled by the symptom you would notice rather than by the cause you do
not know yet.

Upgrading across a version that refuses something it used to accept:
[MIGRATING.md](MIGRATING.md).

## Asking a question

Open a [GitHub issue](https://github.com/Neoforge-Labs/ligase/issues) —
for a question as much as for a bug. Please search the existing ones first.

There is no mailing list, no chat and no commercial support. This is a small
project; one person answers.

## Reporting a bug

The thing that makes a bug report useful is enough to reproduce it. Please
include:

- the version — `python -c "import ligase; print(ligase.__version__)"`
- your OS and Python version, and whether you installed from PyPI
  (`ligase-bio`), Bioconda, or a source build
- the smallest pipeline that shows it
- **what the engine printed, copied verbatim** — the diagnostic text is the
  most informative part of a report, and a paraphrase loses the part that
  identifies the cause
- what you expected instead

A wrong answer that raised no error is the most valuable report this project
can receive, and the hardest to find. If you have one, say what the inputs
were and what made you notice. If the data cannot be shared, a description of
the dimensions involved — which build, which coordinate convention, which
identifier space — is usually enough to reconstruct the case.

## Security

Please do not open a public issue for a vulnerability. Report it privately
through [GitHub security
advisories](https://github.com/Neoforge-Labs/ligase/security/advisories/new).

## What this software is not for

Ligase is research software. Its drug-resistance calls are **not for clinical
use** — not for diagnosis, not for treatment decisions, and not a substitute
for a validated clinical assay or a clinician's judgement. See
[Intended use](https://github.com/Neoforge-Labs/ligase#intended-use) and
[Validation](https://ligase.neoforgelabs.tech/project/validation/).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
