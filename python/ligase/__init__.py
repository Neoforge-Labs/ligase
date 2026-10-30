"""Ligase: type-safe, reproducible computational biology pipelines.

Biological data types carry dimensions (organism, reference build, coordinate
system, sample set), and the engine refuses to connect steps whose dimensions
do not match -- like units in physics.

    >>> import ligase as lg
    >>> from ligase.types import Alignment, Reads, Reference
    >>> hxb2 = lg.reference("refs/HXB2.fasta", organism="HIV-1", build="HXB2")
    >>> samples = lg.samples("data/*.fastq.gz", paired=True)
    >>> @lg.step(tool="minimap2")
    ... def align(reads: Reads, ref: Reference) -> Alignment: ...
    >>> pipe = lg.Pipeline(samples.map(lambda s: align(s, hxb2)))
    >>> pipe.check()   # type-checks every step; nothing runs
    >>> pipe.run()     # parallel, cached, per-sample fan-out
"""

from __future__ import annotations

from glob import glob
from typing import TYPE_CHECKING, TypeVar

from . import _core, chem, hivdb, remotes
from .audit import AuditReport, StepAudit
from .convert import (
    fan_out_ranked_identifiers,
    map_identifiers,
    map_ranked_identifiers,
)
from .errors import (
    CheckError,
    ContigNamingError,
    ContigOrderError,
    DimensionError,
    ReferenceFrameError,
    StepDefinitionError,
    StepTypeError,
    UndeclaredDimensionError,
)
from .errors import (
    build as _build_errors,
)
from .graph import Pipeline, Report, Step, StepOutput, handle, step
from .provenance import (
    BoundTo,
    Bundle,
    Cell,
    Claim,
    ClaimCheck,
    InputCheck,
    Methods,
    MethodsGap,
    PinnedReference,
    ResultCheck,
    StepMethods,
    Verification,
    cell,
)
from .recipes import Shell, Written, sh, write
from .run import RunError, RunReport, StepFailure, StepRun
from .samples import Samples
from .types import (
    KINDS,
    Alignment,
    Contig,
    Dataset,
    DimensionValue,
    EntitySet,
    Intervals,
    KnowledgeGraph,
    MoleculeRanking,
    MoleculeSet,
    RankedList,
    Reads,
    Reference,
    ResistanceReport,
    Variants,
)

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

#: Which kind of data a call produced, so `lg.remote(kind=Reference)` is
#: typed as a `Reference` rather than as a bare `Dataset`.
DatasetT = TypeVar("DatasetT", bound=Dataset)

__all__ = [
    "Alignment",
    "AuditReport",
    "BoundTo",
    "Bundle",
    "Cell",
    "CheckError",
    "Claim",
    "ClaimCheck",
    "Contig",
    "ContigNamingError",
    "ContigOrderError",
    "Dataset",
    "DimensionError",
    "DimensionValue",
    "EntitySet",
    "InputCheck",
    "Intervals",
    "KnowledgeGraph",
    "LoadError",
    "Methods",
    "MethodsGap",
    "MoleculeRanking",
    "MoleculeSet",
    "PinnedReference",
    "Pipeline",
    "RankedList",
    "Reads",
    "Reference",
    "ReferenceFrameError",
    "RemoteError",
    "Report",
    "ResistanceReport",
    "ResultCheck",
    "RunError",
    "RunReport",
    "Samples",
    "Shell",
    "Step",
    "StepAudit",
    "StepDefinitionError",
    "StepFailure",
    "StepMethods",
    "StepOutput",
    "StepRun",
    "StepTypeError",
    "UndeclaredDimensionError",
    "Variants",
    "Verification",
    "Written",
    "__version__",
    "alignment",
    "cell",
    "check_step",
    "chem",
    "dimension_errors",
    "entity_set",
    "fan_out_ranked_identifiers",
    "handle",
    "hivdb",
    "intervals",
    "knowledge_graph",
    "map_identifiers",
    "map_ranked_identifiers",
    "molecule_ranking",
    "molecule_set",
    "query",
    "ranked_list",
    "reads",
    "reference",
    "remote",
    "samples",
    "sh",
    "step",
    "steps",
    "variants",
    "version",
    "write",
]

#: Version of the compiled core engine. Reported by Rust, never hardcoded.
__version__: str = _core.version()

#: Raised when a file cannot be read or does not parse as its format.
LoadError = _core.LoadError

#: Raised when a remote cannot be fetched, or is not what its digest says.
#:
#: Its own class rather than a `LoadError`, because the two ask different
#: things of a reader: a `LoadError` is about a file that is wrong, and this
#: is about a *source* that is unreachable, has moved, or served something
#: other than what was asked for.
RemoteError = _core.RemoteError


def version() -> str:
    """Return the version of the compiled Ligase core engine."""
    return _core.version()


# Imported last, and deliberately. The step library's modules decorate
# their functions with `ligase.graph.step` at import time, so importing
# `steps` from the top of this file would be asking a half-initialised
# package for a name it has not bound yet.
from . import steps  # noqa: E402

# -------------------------------------------------------------- loaders --


def reference(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
) -> Reference:
    """Load a reference FASTA, inferring its contigs.

    Each contig's MD5 is computed here, over the uppercased sequence, as the
    SAM specification defines `@SQ M5`. Computing it ourselves is what lets
    us check an aligner's claim about which reference it used.

    Args:
        path: The FASTA, optionally gzipped.
        organism: The organism, e.g. `HIV-1`. A FASTA cannot record this.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label such as `HXB2` or `hg38`. Display only --
            reference identity never rests on a label, so two files both
            labelled `hg38` still fail to match if their contigs disagree.
        frame: Which coordinate frame this reference's positions are
            numbered along, as `(frame, first)`: base 1 of this file is
            base `first` of `frame`, 1-based, so a whole genome declares
            `("K03455", 1)` and the usual pol slice declares
            `("K03455", 2085)`.

            Declared because no FASTA records it. A 3,012-base slice of
            K03455 and a 3,012-base genome of something else are identical
            from the outside, and a step whose own coordinates are fixed --
            `lg.hivdb.interpret` holds the HIV-1 gene windows as positions
            in K03455 -- cannot tell them apart without being told. Leave
            it unset for a whole-genome reference such as HXB2 itself,
            which is recognised by its length; a step that needs a frame
            and cannot work one out asks for it by name.
        release: Which release of the reference this is, e.g.
            `GRCh38.p14` or `Ensembl 110`. A FASTA cannot record it, and
            two releases of one build can differ in sequence while sharing
            a name. It is compared by equality: two references declare the
            same release only if their labels are identical after
            trimming, so `GRCh38.p13` and `GRCh38.p14` do not match.

    Returns:
        The loaded reference.

    Raises:
        ValueError: If `organism` names no organism (an empty list, or only
            blank names), or `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a FASTA.
    """
    return Reference(_core.load_reference(path, organism, build, frame, release))


def alignment(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Alignment:
    """Load a SAM or BAM, inferring dimensions from its header.

    Both containers yield identical dimensions: a lossless
    ``samtools view -b`` changes nothing biological.

    Args:
        path: The alignment file.
        organism: The organism. SAM and BAM headers do not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the `@RG SM` tags.

    Returns:
        The loaded alignment.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a SAM/BAM.
    """
    return Alignment(_core.load_alignment(path, organism, build, samples))


def variants(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Variants:
    """Load a VCF, inferring dimensions from its header.

    Identity comes from `##contig` alone. `##reference` is deliberately not
    trusted: it holds a path, and two runs pointing at `/refs/genome.fa` can
    be pointing at different genomes.

    Args:
        path: The VCF, optionally gzipped or BGZF-compressed.
        organism: The organism. A VCF does not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the header columns.

    Returns:
        The loaded variant calls.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a VCF.
    """
    return Variants(_core.load_variants(path, organism, build, samples))


def reads(
    r1: str,
    r2: str | None = None,
    *,
    organism: str | Sequence[str] | None = None,
    sample: str | None = None,
) -> Reads:
    """Load sequencing reads, inferring what a FASTQ can tell us.

    The quality encoding comes from the observed score range, and comes back
    unknown rather than guessed when the scores fall in the range Phred+33
    and Phred+64 share. Guessing would shift every quality value by 31,
    which no downstream tool would notice.

    Args:
        r1: The reads, or the first mate of a pair. Optionally gzipped.
        r2: The second mate, if the reads are paired.
        organism: The organism. A FASTQ does not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        sample: The sample identifier, overriding the one taken from the
            filename.

    Returns:
        The loaded reads.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If a file cannot be read or is not a FASTQ.
    """
    samples = None if sample is None else [sample]
    return Reads(_core.load_reads(r1, r2, organism, samples))


def intervals(
    path: str,
    *,
    format: str | None = None,  # noqa: A002 - the file's format, not a builtin
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
) -> Intervals:
    """Load genomic intervals from a BED or GFF, inferring what they say.

    The coordinate convention comes from the format — BED is 0-based
    half-open, GFF and GTF are 1-based closed — and that is the point: a
    tool handed the wrong one produces results that are off by one base and
    look fine.

    Contigs come from the records, so they carry no length and no checksum.
    They are still compared against a reference by name, because comparing
    what is present is what makes these files checkable at all.

    Args:
        path: The BED, GFF3 or GTF, optionally gzipped.
        format: `bed`, `gff`, `gff3` or `gtf`, when the filename does not
            say. A format is never guessed: guessing it wrong shifts every
            interval by a base.
        organism: The organism. An interval file does not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.

    Returns:
        The loaded intervals.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, is not in the format given,
            or its format cannot be told from its name.
    """
    return Intervals(_core.load_intervals(path, format, organism, build))


def entity_set(
    path: str,
    *,
    id_column: str = "id",
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> EntitySet:
    """Load a set of genes or proteins from a TSV with a header.

    The first type here that holds no sequence. What it carries instead is
    the identifier space its names live in, which is the dimension it exists
    for: a list of HGNC symbols joined to a table of Ensembl IDs overlaps in
    nothing, and until now nothing said so.

    Args:
        path: The TSV, optionally gzipped. It must have a header row.
        id_column: The column holding the identifiers. Every other column is
            an attribute, carried without being read.
        organism: The organism these entities belong to. A gene list does
            not record it. Give a list for data about more than one
            organism; two sets of organisms are equal when they name the
            same ones, whatever the order.
        identifier_namespace: Which space the identifiers are in, such as
            `HGNC symbol`, `Ensembl gene`, `Entrez gene` or
            `UniProt accession` -- or whatever an in-house list calls its
            own spelling, since a curated set's names are a space too.

            Leave it out and the space is read off the identifiers
            themselves, but only where every one of them matches a single
            known pattern: a file mixing two spaces, a file with no rows,
            and a file holding one name the engine cannot place all come
            back with the dimension unknown rather than guessed. Declared,
            it is *recorded* rather than checked -- the engine knows seven
            spaces and there are more than seven, so it cannot tell a space
            it has never heard of from a wrong declaration.
        subject: What the list is about when that is one named thing: a
            disease, a cell line, a cohort. Compared by equality.
        release: Which release named these entities, such as
            `HGNC 2026-09` or `Ensembl 110`. Compared by equality, so
            `HGNC 2026-03` and `HGNC 2026-09` do not match -- which is the
            point, because a symbol can mean different genes in two
            releases.

    Returns:
        The loaded set. Anything the loader noticed about the identifiers --
        a symbol a spreadsheet has turned into a date, say -- is on `notes`.

    Raises:
        ValueError: If `organism` names no organism (an empty list, or only
            blank names), or if `identifier_namespace`, `subject` or
            `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, has no header row, has no
            column named `id_column`, or holds a row its header cannot
            account for.
    """
    return EntitySet(
        _core.load_entity_set(
            path, id_column, organism, identifier_namespace, subject, release
        )
    )


def ranked_list(
    path: str,
    *,
    id_column: str = "id",
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> RankedList:
    """Load a ranked list from a TSV with a header, highest rank first.

    An `entity_set` and one more thing: the row order is the result. Rank is
    the row position, so a ranking reordered is a different ranking and has
    a different semantic hash -- where every other type in Ligase, this
    one's own unranked sibling included, treats record order as saying
    nothing. Ranked differentially-expressed genes, ranked drug targets and
    a leaderboard of candidates are all this type.

    Args:
        path: The TSV, optionally gzipped. It must have a header row, and
            its rows must be in rank order: best first.
        id_column: The column holding the identifiers. Every other column is
            an attribute, carried without being read -- a `score`, a `rank`,
            an adjusted p-value. A ranked table often has a rank column of
            its own, so this is more often worth naming than for a gene
            list.
        organism: The organism these entities belong to. A ranked list does
            not record it. Give a list for data about more than one
            organism.
        identifier_namespace: Which space the identifiers are in, such as
            `HGNC symbol` or `Ensembl gene` -- or whatever an in-house list
            calls its own spelling. Left out, it is read off the identifiers
            themselves where every one of them matches a single known
            pattern, exactly as for `entity_set`.
        subject: What the ranking is of when that is one named thing: a
            disease, a cell line, a cohort. Compared by equality.
        release: Which release named these entities, such as
            `HGNC 2026-09`. Compared by equality, so a ranking named by one
            release will not score against a list named by another -- which
            is the point, because a symbol can mean different genes in two
            releases.

    Returns:
        The loaded ranking. Anything the loader noticed about the
        identifiers -- a symbol a spreadsheet has turned into a date, say --
        is on `notes`.

    Raises:
        ValueError: If `organism` names no organism (an empty list, or only
            blank names), or if `identifier_namespace`, `subject` or
            `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, has no header row, has no
            column named `id_column`, or holds a row its header cannot
            account for.

    Note:
        A `score` column is compared **exactly**. No numeric tolerance is
        declared for one, because none has been measured: a tolerance
        guessed at would be baked into every cache key, and a cache that is
        too cold costs time while one that is too warm is wrong.
    """
    return RankedList(
        _core.load_ranked_list(
            path, id_column, organism, identifier_namespace, subject, release
        )
    )


def knowledge_graph(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
    source_set: Sequence[str] | None = None,
) -> KnowledgeGraph:
    """Load a knowledge graph of typed nodes and edges from JSON.

    The first type here that is *assembled* rather than measured, and what
    follows from that is the dimension it exists for. What a graph holds is
    a function of which evidence sources were folded into it: add DisGeNET
    to a graph that had Open Targets and a literature-mined set and edges
    appear, neighbourhoods widen and every propagated score moves. Nothing
    in the file says which sources went in, so two graphs that differ only
    in that diff and score against each other without complaint, and the
    difference reads as biology.

    So the sources are a dimension, and this loader reads them off the
    graph's own nodes and edges rather than asking you for them.

    The JSON is one object with a `nodes` array and an `edges` array. A node
    needs a string `id`; an edge needs a string `source`, `target` and
    `type`. A `sources` list on a node, on an edge, or at the top level is
    the per-edge provenance the dimension is read from, and a node's `ids`
    are its other names. Everything else either carries is kept without
    being read, exactly as a gene list's attribute columns are.

    Args:
        path: The JSON file, optionally gzipped.
        organism: The organisms the graph's entities belong to. A graph does
            not record them, and a graph joining human genes to evidence
            from model organisms is legitimately about several -- give a
            list for that.
        identifier_namespace: Which space the node ids are in, such as
            `Ensembl gene` or `HGNC symbol`. Left out, it is read off the
            node ids where every one of them matches a single known
            pattern; a graph holding more than one kind of entity will not,
            which is ordinary rather than a fault, and leaves the dimension
            unknown rather than guessed.
        subject: What the graph is a graph of when that is one named thing:
            a disease, a cohort, a therapeutic area. Compared by equality.
        release: Which release the sources were drawn from, such as
            `Open Targets 24.09`. Compared by equality, so 24.06 and 24.09
            do not match -- which is the point.
        source_set: The sources the graph was assembled from. Left out, the
            loader reads them off the graph. Declare it where the graph's
            own names are a mirror, a shard or an internal pipeline stage
            rather than the source a reader would recognise. Compared
            **exactly**, case included: `s3://bucket/A.json` and
            `s3://bucket/a.json` are two objects by specification.

    Returns:
        The loaded graph.

    Raises:
        ValueError: If `organism` or `source_set` names none (an empty list,
            or only blank names), or if `identifier_namespace`, `subject` or
            `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, is not JSON, has no `nodes`
            or `edges` array, or holds a node with no `id` or an edge with
            no `source`, `target` or `type`.

    Note:
        A graph naming no sources anywhere leaves `source_set` **unknown**,
        never an empty set. Two graphs that each named none would otherwise
        agree on the axis, which is exactly the silence the dimension exists
        to break.
    """
    return KnowledgeGraph(
        _core.load_knowledge_graph(
            path, organism, identifier_namespace, subject, release, source_set
        )
    )


def molecule_set(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    representation: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> MoleculeSet:
    """Load a set of molecules from a tab-separated table.

    The type the `representation` dimension exists for. The same substance
    has different InChIKeys depending on what was done to it before the key
    was computed: of thirty ordinary drugs measured across six RDKit
    releases, twelve change key under a different standardisation state,
    and aspirin has three. So two compound sets prepared differently
    overlap in nothing for the substances affected while the rest still
    match, and what comes out is a hit rate that is small, non-zero and
    reads as poor enrichment rather than as a mismatch.

    Nothing about two columns of twenty-seven-character keys says which
    standardisation produced them. That is what makes this the one
    dimension in Ligase no glance at the data can catch, and why the engine
    refuses to join two sets that disagree about it.

    The table needs an `inchikey` column or a `smiles` column. Every other
    column is carried without being read, a score or an activity included.
    A `#ligase representation:` line above the header is where the
    dimension is read from, and `lg.chem.standardize` is what writes one.

    Args:
        path: The tab-separated table, optionally gzipped.
        organism: The organisms the compounds were measured against. A
            compound table does not record them.
        representation: How this table's names were arrived at, spelled as
            `inchikey; salt-stripped, charge-neutralised` — an identity,
            then the standardisations applied, or `as given` for none.
            Left out, it is read off the table's own representation line;
            a table of InChIKeys with no such line leaves the dimension
            **unknown**, never `as given`, because a ChEMBL download is
            standardised and a vendor list is not, and assuming the weaker
            state would be inventing the most dangerous of the possible
            answers.
        subject: What the set is of when that is one named thing: a target,
            an assay, a therapeutic area. Compared by equality.
        release: Which release of a compound collection this came from,
            such as `ChEMBL 34`. Compared by equality, so 34 and 35 do not
            match — which is the point.

    Returns:
        The loaded set.

    Raises:
        ValueError: If `organism` names none, if `subject` or `release` is
            blank, or if `representation` is not a spelling the engine can
            read.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, has no header row, has
            neither identity column, holds a row its header cannot account
            for, or carries a representation line naming a standardisation
            the engine does not know.

    Note:
        A declared representation is **recorded, not verified**. The engine
        has no chemistry — computing an InChIKey needs RDKit, which is an
        optional extra and not a dependency of the core — so it cannot
        check the claim. `lg.chem.standardize` checks its own work instead,
        which is the half that can be checked.
    """
    return MoleculeSet(
        _core.load_molecule_set(path, organism, representation, subject, release)
    )


def molecule_ranking(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    representation: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> MoleculeRanking:
    """Load a ranking of molecules, whose row order is the answer.

    Everything `lg.molecule_set` reads, with one difference that is the
    whole reason the type exists: the canonical form **keeps the row
    order**. A ranking of docked compounds answers which of them to order,
    rank is the row position, and the same ten thousand compounds in two
    orders are two different answers. A cache that sorted them would serve
    one for the other in silence, and for a virtual screen the order
    decides the length of the prefix anybody actually buys.

    A score column is optional and compared exactly, with no tolerance: no
    drift has been measured for one, and a guessed tolerance would be baked
    into every cache key below it.

    Args:
        path: The tab-separated table, optionally gzipped, in rank order.
        organism: As `lg.molecule_set`.
        representation: As `lg.molecule_set`.
        subject: As `lg.molecule_set` — for a ranking this is usually the
            target the compounds were scored against.
        release: As `lg.molecule_set`.

    Returns:
        The loaded ranking.

    Raises:
        ValueError: As `lg.molecule_set`.
        TypeError: As `lg.molecule_set`.
        LoadError: As `lg.molecule_set`.
    """
    return MoleculeRanking(
        _core.load_molecule_ranking(path, organism, representation, subject, release)
    )


def remote(
    locator: str,
    *,
    kind: type[DatasetT],
    digest: str | None = None,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
    samples: list[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    source_set: Sequence[str] | None = None,
    representation: str | None = None,
) -> DatasetT:
    """Fetch a pinned remote input, and load what arrives.

    The first step of nearly every pipeline is a fetch, and until now it sat
    outside everything this engine promises: a shell script downloaded some
    reads and the engine took its word for what had arrived. A remote
    declared here is an input like any other, with dimensions read from its
    own bytes and a content hash in every cache key below it.

    **A pinned remote promises the same bytes forever, from the source.** An
    SRA run, an NCBI accession, a numbered release: the archive undertakes
    not to change them, so anyone, anywhere, at any time can obtain this
    input and check it. That is what makes `digest=` worth writing, and why
    a digest that does not match is a loud failure rather than a quiet
    re-fetch. A source that promises nothing needs the other half of this
    design, a recorded query, and is not what this function is for.

    ## When it happens, and what it costs

    It resolves when you call it, while the pipeline is still being read.
    That is not an optimisation to be deferred: a remote's dimensions cannot
    be known until its bytes are in hand, and the engine checks dimensions
    before any compute. So a wrong digest fails before any step runs,
    because the failure happens before the graph is even built.

    On a second run nothing is fetched. The locator and its digest are in
    the store, the objects are there, and resolution is a lookup -- which is
    why a pipeline built on remotes runs offline once it has run online.

    ## What `digest` is over

    What the file *means*, not what its bytes are: the canonical form of
    what arrives, read as `kind`, folded over its files in order. A source
    that reflows a FASTA or rewords a defline has not changed the biology,
    and a pin that broke on that would teach you to bump pins without
    reading -- after which the one that matters gets bumped too. Such a
    rewrite is reported as a note instead. A change in what the data
    *means* is the failure.

    Leave it out on the first run: the digest of what arrived is reported
    and recorded, and you paste it in to make every later run a checked one.

    Args:
        locator: Where it comes from, as one of four schemes.
            `sra:SRR32217726` is an SRA run, served by ENA as a pair of
            gzipped FASTQs. `ncbi:K03455.1` is an NCBI nucleotide
            accession, fetched as FASTA. `ensembl:release-110/...` is a path
            under Ensembl's public tree. `https://...` is an exact URL, for
            everything else. Anything else is an error naming the four.
        kind: What the bytes are, as one of the types in `ligase.types`:
            `Reference` for a FASTA, `Reads` for a FASTQ pair. Required,
            and not inferred from the locator, because it decides how the
            file is read and therefore what its digest is over -- and
            because it is what the result is typed as, so a step annotated
            `Reference` will not accept reads by mistake.
        digest: The digest this input must have, as the 64 hexadecimal
            characters a previous run reported. `None` fetches whatever the
            source serves and records its digest, which is how you get the
            value to pin.
        organism: The organism, as for the matching loader. A fetched file
            records no more about itself than a local one does.
        build: A human label for the reference build. Display only.
        frame: Which coordinate frame a reference's positions are numbered
            along, as `(frame, first)`.
        release: Which release of the data this is. Compared by equality.
        samples: The sample identifiers. **Required for reads**, and only
            for them: a local FASTQ's sample comes from its filename, and a
            fetched one is stored under the hash of its bytes, which is no
            sample identifier at all. The run accession is usually what you
            want, but say so rather than have the engine assume it -- one
            sample can have several runs.
        identifier_namespace: Which identifier space an entity set's names
            live in.
        subject: What the data is about, when that is one named thing.
        source_set: The sources a knowledge graph was assembled from. Left
            out, they are read off the graph's own nodes and edges.
        representation: How a molecule table's names were arrived at,
            spelled as `inchikey; salt-stripped, charge-neutralised`. Left
            out, it is read off the table's own representation line -- and
            a downloaded table of InChIKeys usually has none, so this is
            the fetch that most often needs it.

    Returns:
        The loaded data, of the class `kind` named -- so the call is typed
            as a `Reference` or a `Reads` and not as a bare dataset --
            carrying the dimensions
        inferred from what arrived and a record of where it came from. That
        record is what puts the fetch in the methods paragraph and the
        reproducibility bundle.

    Raises:
        RemoteError: If the locator names no scheme Ligase has, if the
            source cannot be reached -- which includes a run configured
            offline by `ligase.remotes.configure` whose store does not hold
            this input -- if what arrives cannot be read as `kind`, since
            an archive asked for an accession it does not have answers with
            an error page and a 200, or if the digest does not match.
        ValueError: If `kind` is not a kind of data Ligase knows, if
            `digest` is not 64 hexadecimal characters, if `kind` is `Reads`
            and no `samples` are named, or if a declared dimension is
            blank.
        LoadError: If what arrives cannot be parsed.
    """
    named = kind.__name__
    if named not in KINDS:
        known = ", ".join(sorted(KINDS))
        message = f"`{named}` is not a kind of data Ligase knows. The kinds are {known}"
        raise ValueError(message)

    return kind(
        _core.resolve_remote(
            locator,
            named,
            digest,
            str(remotes.cache_directory()),
            remotes.offline(),
            organism,
            build,
            frame,
            release,
            samples,
            identifier_namespace,
            subject,
            list(source_set) if source_set is not None else None,
            representation,
        )
    )


def query(
    locator: str,
    *,
    kind: type[DatasetT],
    auth_env: str | None = None,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
    samples: list[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    source_set: Sequence[str] | None = None,
    representation: str | None = None,
) -> DatasetT:
    """Record a live query's answer, or replay the answer already recorded.

    The other half of `remote`, for a source that promises nothing. An
    Open Targets query will answer differently next quarter, because it is
    supposed to -- new evidence arrives -- and a pipeline that re-asked it
    on every run would quietly stop being the pipeline that produced the
    paper.

    **A recorded query promises the same bytes forever, from the record.**
    Not from the source, which is the promise `remote` makes and this one
    cannot. What makes that enough is that the canonicalised response
    **body** goes into the content-addressed store rather than merely its
    digest: the body is then an ordinary input with a content hash,
    everything below it is deterministic by construction, and a reviewer
    replays the whole run with no network and no API key.

    ## The three modes, which are a setting for the run

    | Mode | What happens |
    |---|---|
    | `auto` | replay a record that exists, record one that does not. The default |
    | `replay` | replay only: a query with no record is an **error** |
    | `refresh` | ask the source anyway, and report what changed |

    Set by `ligase run --replay` and `--refresh`, or by
    `ligase.remotes.configure(mode=...)`, and set for the run rather than
    for the step -- because a run that replayed some of its queries and
    fetched the rest would be reporting a result nobody can repeat. In
    `replay` a query with no record is refused rather than fetched, and the
    run report always says which of the three each query did.

    A refresh never overwrites a record. It writes a new dated one beside
    the old, so a pipeline refreshing weekly is a series of runs, each
    replayable forever.

    ## What identifies a record

    The request, not the order it was asked in: the method, the URL with its
    parameters sorted, and the body. A step making twenty paginated calls
    therefore writes twenty records and finds them all again next time
    whatever order it asks in.

    Args:
        locator: Where to ask, as one of the four schemes -- in practice
            `https://...`, since a query is a question and the other three
            name archives. Parameters may be in any order: two URLs
            differing only in that are one record.
        kind: What the answer is, as one of the types in `ligase.types`.
            Required, and not inferred, because it decides how the body is
            read and therefore what the digest is over. An answer that will
            not read as `kind` is refused rather than recorded: an API asked
            for something it does not have answers with an error page and a
            200.
        auth_env: The name of an environment variable holding a bearer
            token, sent as `Authorization: Bearer <value>`. The **name**,
            not the token: a bundle carries the pipeline file, so a token
            written in the pipeline would travel in every bundle. Nothing
            the engine stores has a field for a header, so the record and
            the bundle hold no trace of it.
        organism: The organism, as for the matching loader.
        build: A human label for the reference build. Display only.
        frame: Which coordinate frame a reference's positions are numbered
            along, as `(frame, first)`.
        release: Which release of the data this is. Compared by equality.
        samples: The sample identifiers. Required for reads.
        identifier_namespace: Which identifier space an entity set's names
            live in. Worth declaring for a query, because an API's own
            namespace is rarely the one the rest of your pipeline uses.
        subject: What the data is about, when that is one named thing.
        source_set: The sources a knowledge graph was assembled from. Left
            out, they are read off the graph's own nodes and edges -- which
            for a query means off the answer the service gave.
        representation: How a molecule table's names were arrived at,
            spelled as `inchikey; salt-stripped, charge-neutralised`. A
            service that answers with compounds rarely says what
            standardised them, so this is usually the only way to state it.

    Returns:
        The loaded data, of the class `kind` named, carrying the dimensions
            inferred from the recorded body and a record of where it came
            from -- which is what puts the query in the methods paragraph
            and its response in the
        reproducibility bundle.

    Raises:
        RemoteError: If the locator names no scheme Ligase has, if the
            source cannot be reached, if this run may only replay and there
            is no record of this query, if the answer is larger than the 32
            MiB a record may hold -- an answer that large is a dataset, and
            a dataset has a version to pin with `remote` instead -- if the
            answer cannot be read as `kind`, or if `auth_env` names a
            variable that is not set.
        ValueError: If `kind` is not a kind of data Ligase knows, or a
            declared dimension is blank.
        LoadError: If the recorded body cannot be parsed.
    """
    named = kind.__name__
    if named not in KINDS:
        known = ", ".join(sorted(KINDS))
        message = f"`{named}` is not a kind of data Ligase knows. The kinds are {known}"
        raise ValueError(message)

    return kind(
        _core.resolve_query(
            locator,
            named,
            remotes.mode(),
            str(remotes.cache_directory()),
            auth_env,
            organism,
            build,
            frame,
            release,
            samples,
            identifier_namespace,
            subject,
            list(source_set) if source_set is not None else None,
            representation,
        )
    )


def samples(
    pattern: str,
    *,
    paired: bool = False,
    organism: str | Sequence[str] | None = None,
) -> Samples:
    """Find sequencing runs and group them into samples.

    Args:
        pattern: A glob, e.g. `data/*.fastq.gz`. Matches are taken in sorted
            order, so the samples come back in a stable order on every
            machine.
        paired: Whether the files are mates of a pair. When true, they are
            paired by filename — `<sample>_1`/`<sample>_2` or
            `<sample>_R1`/`<sample>_R2` — and a file whose mate cannot be
            identified is an error rather than a guess. Mis-pairing two
            samples' files would produce results for a mixture of two
            people without anything crashing.
        organism: The organism. A FASTQ does not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.

    Returns:
        The samples, in the order their files appeared. Use `.map()` to
        apply a chain of steps to every one of them.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If a file cannot be read, or the files cannot be grouped
            into whole samples.
    """
    found = sorted(glob(pattern))
    return Samples(
        [Reads(core) for core in _core.load_samples(found, paired, organism)]
    )


# -------------------------------------------------------------- checking --


def dimension_errors(step: str, bindings: Mapping[str, Dataset]) -> list[CheckError]:
    """Return every dimension disagreement among a step's inputs.

    One step on its own, for when you are not building a pipeline. A whole
    pipeline is checked with `lg.Pipeline(...).check()`, which also knows
    what each step requires of the ones before it.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Returns:
        One error per disagreement, most general first. Empty if the step's
        inputs are consistent. Every error carries the full list in its
        `errors` attribute.
    """
    inputs = [(name, data._core) for name, data in bindings.items()]
    return _build_errors(_core.check_step(step, inputs))


def check_step(step: str, bindings: Mapping[str, Dataset]) -> None:
    """Check a step's inputs, raising on the first disagreement.

    Every disagreement is found before anything is raised, so the raised
    error's `errors` attribute holds all of them. Nothing is computed.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Raises:
        CheckError: If any two inputs disagree. The subclass says what kind
            of disagreement it is: `ContigNamingError`, `ContigOrderError`
            or `UndeclaredDimensionError`, all of which are
            `DimensionError`s.
    """
    errors = dimension_errors(step, bindings)
    if errors:
        raise errors[0]
