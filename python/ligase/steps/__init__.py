"""Steps the project ships, and what one has to say for itself to be here.

`ligase.steps` is a curated library, not a convenience wrapper. The
difference matters, because a wrapper's only job is to save typing and this
library's job is to be *true*:

    from ligase.steps import bcftools, minimap2

    aligned = minimap2.align(reads, hxb2, sample_id="SRR8525904")
    called = bcftools.mpileup_call(aligned, hxb2, min_freq=0.01)

Those two lines are the whole of the reference demo's compute, and the demo
used to spell them out as two `lg.sh` commands of its own. It no longer does,
which is the argument for the library existing: a pipeline should look like
what it means, and the twelve-line shell incantation that gets a read group
onto a BAM is not what the pipeline means.

## What a step must say to be admitted (D42)

A step is in this library only if it declares, and is tested on, four
things. None of them is about the tool being popular.

**Its tool's version capture.** `tool=` names every program the body runs,
and each one's version joins the cache key, so an upgrade that could change
a result reruns the steps it could have changed. A tool that does not answer
`--version` needs an entry in the engine's version-probe table before it can
be named here: `bwa` is the reason that table exists, because
`bwa --version` prints a complaint and exits non-zero, and recording that
complaint as a version meant the key never moved when the aligner did.

**Its citation.** Every tool a library step names resolves to a reference in
the engine's citation table, so `ligase methods` prints a reference list a
reviewer can check. A user's own step declares what it wants cited with
`cite=`; a step the project ships does not get to take that liberty on the
project's behalf, so the reference is curated instead (D24).

**The dimensions of its output.** Which the step mostly does not *choose* —
the engine propagates dimensions from the inputs — but which it must not
lose. Both aligners here take a `sample_id` for exactly this reason: neither
writes a read group of its own, a BAM without one has no sample set, and a
sample set is a dimension this engine exists to keep straight.

**How that output canonicalises.** Every kind in Ligase has a canonical
form, so this is a statement about the *file*: it has to parse as the kind
the step says it is. A step whose output does not parse is not an error —
the engine falls back to hashing its bytes — which is precisely why it has
to be checked here, because the fallback is silent and costs early cutoff
everything below that step.

**A step that cannot say what it produces is not a library step.** That
clause has already done more work than the four above it. See the
[architecture page](https://ligase.neoforgelabs.tech/project/architecture/step-library/)
for the two steps it has kept out and why — `samtools index`, whose output
is an index and not an alignment, and `fastp`'s paired-end trimming, which
no step can produce because a step's output is one file and paired reads are
two.

## What is here

| Step | In → Out |
|---|---|
| `minimap2.align` | `Reads, Reference → Alignment` |
| `bwa.mem` | `Reads, Reference → Alignment` |
| `samtools.sort` | `Alignment → Alignment` |
| `bcftools.mpileup_call` | `Alignment, Reference → Variants` |

Two aligners producing one kind is deliberate. They are not
interchangeable in provenance — different programs, different versions,
different references — so swapping one for the other is a different cache
key and a different methods paragraph, and that falls out of `tool=` rather
than being arranged.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from . import bcftools, bwa, minimap2, samtools

if TYPE_CHECKING:
    from ..graph import Step

__all__ = ["LIBRARY", "LibraryStep", "bcftools", "bwa", "minimap2", "samtools"]


@dataclass(frozen=True, slots=True)
class LibraryStep:
    """One entry in the library, and what it claims about itself.

    The claims are kept as data rather than as prose so that one test can
    iterate them. An admission criterion nothing checks is a comment, and
    the four in this module's docstring are meant to hold a contributor who
    has not read it.

    Attributes:
        step: The step itself, as `@lg.step` made it. Its `tools`, its
            `output` kind and its own signature are read from here rather
            than repeated.
        module: The dotted module it lives in, which is also how a user
            writes it: `ligase.steps.minimap2`.
        canonical: The canonical form the output takes, which is
            `"canonical"` for every step here — meaning the file parses as
            the kind the step declares, rather than falling back to its
            bytes.
        page: The guide page that documents it, as a site path. Checked,
            because the clause that every library step has one is only a
            promise while something fails when it does not.
    """

    step: Step[Any, Any]
    module: str
    canonical: str
    page: str

    @property
    def name(self) -> str:
        """How the step is written, module and all: `minimap2.align`."""
        return f"{self.module.rsplit('.', 1)[-1]}.{self.step.name}"


#: Every step this project ships, with what each one claims.
#:
#: The order is the order a pipeline meets them: align, align the other way,
#: sort something that came from elsewhere, call.
LIBRARY: tuple[LibraryStep, ...] = (
    LibraryStep(
        step=minimap2.align,
        module="ligase.steps.minimap2",
        canonical="canonical",
        page="/guide/steps/minimap2/",
    ),
    LibraryStep(
        step=bwa.mem,
        module="ligase.steps.bwa",
        canonical="canonical",
        page="/guide/steps/bwa/",
    ),
    LibraryStep(
        step=samtools.sort,
        module="ligase.steps.samtools",
        canonical="canonical",
        page="/guide/steps/samtools/",
    ),
    LibraryStep(
        step=bcftools.mpileup_call,
        module="ligase.steps.bcftools",
        canonical="canonical",
        page="/guide/steps/bcftools/",
    ),
)
