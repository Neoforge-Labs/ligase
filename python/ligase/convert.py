"""Crossing a dimension, in the one way the engine allows.

A dimension check refuses a join. For most dimensions there is nothing past
the refusal: two files about different organisms are about different
organisms. For a few there is a real operation, and this is where those
live.

    ensembl = lg.map_identifiers(
        genes,
        table=lg.entity_set(
            "hgnc_to_ensembl.tsv",
            organism="Homo sapiens",
            identifier_namespace="HGNC symbol",
            release="HGNC 2026-09",
        ),
        column="ensembl_gene_id",
        to="Ensembl gene",
    )

## Why a registry rather than a function

The engine keeps these as a table, not as three functions somebody calls.
Each entry says which axis it crosses, which kinds it crosses it for, and
what the user must supply, so that a search over them can answer "what gets
me from here to there?" — which is what
[D5](https://ligase.neoforgelabs.tech/project/decisions/)'s conversion graph
is for, and what v0.3's automatic fixes will be.

v0.2 registers one conversion. `liftover` (reference build to reference
build) and `rename_contigs` (contig naming to contig naming) are the next
two entries, and the registry is shaped so that they are rows rather than
redesigns: both cross an axis, and both will read their destination from the
same `to=` parameter this one does.

## Why the mapping table is typed

`table=` takes an `EntitySet`, not a path. Two things follow from that, and
both are the point.

Its contents join the cache key, so editing the table reruns the mapping —
which a path would not do, because a plain parameter is hashed by its value
and a path's value does not change when the file behind it does.

And its own `identifier_namespace` is checked against the list being mapped.
A table of Entrez IDs handed to a list of HGNC symbols is a
`DimensionError` before anything runs, rather than a mapping that quietly
matches nothing. The table's identifiers are the *source* side of the
mapping, so they are in the same space as the list; the targets live in an
attribute column, named by `column=`.

## What a conversion is not

It is not a rename. Mapping between identifier spaces is a lookup *with
failures*, and what happens to those failures is the whole argument of
[D38](https://ligase.neoforgelabs.tech/project/decisions/): an identifier
that does not cross is named, never dropped in silence and never carried
over unchanged. Carrying it over would put a name from the old space into a
set that declares the new one, which is the implicit rename the dimension
exists to refuse.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from . import _core
from .graph import step
from .recipes import write
from .types import EntitySet, RankedList

if TYPE_CHECKING:
    from .recipes import Noticed

__all__ = ["map_identifiers", "map_ranked_identifiers"]


@step(tool="ligase", conversion="map_identifiers")
def map_identifiers(
    entities: EntitySet,
    table: EntitySet,
    *,
    column: str,
    to: str,
    id_column: str = "id",
    table_id_column: str = "id",
) -> EntitySet:
    """Rewrite a gene list's identifiers through a mapping table.

    The output declares the space named by `to`, and carries every other
    dimension of `entities` — its organism, its subject, its release —
    because a conversion crosses one axis and is not a new list.

    Args:
        entities: The list to convert.
        table: The mapping, as an `EntitySet` whose own identifiers are the
            *source* side. Typed rather than a path so that its contents
            join the cache key and its space is checked against
            `entities`: a table of Entrez IDs handed a list of HGNC symbols
            is refused before anything runs.
        column: The column of `table` holding the identifiers to convert
            *to*. Naming the table's own identifier column is an error — it
            would map every identifier to itself and then declare a space
            the identifiers are not in.
        to: The space the output is in, such as `Ensembl gene`. Declared
            rather than inferred: what the output *is* must not depend on
            which identifiers happened to cross.
        id_column: The column of `entities` holding its identifiers.
        table_id_column: The column of `table` holding the source
            identifiers.

    Returns:
        A recipe, typed as the gene list the step produces.

    Note:
        Two kinds of identifier do not cross, and both are named in the
        run report, provenance, a bundle and the methods section rather
        than being dropped quietly. One the table has no row for is usually
        a symbol renamed or retired since the table was downloaded. One the
        table sends to several targets — ordinary for symbol-to-gene
        mappings — is left out because emitting a row for each would make
        the output larger than the input without saying so, and choosing
        one would be a guess about which the analysis meant.

        So the output is smaller than the input by exactly the identifiers
        the notes name.
    """

    def produce(out: Path) -> list[Noticed]:
        """Map and write, once the steps above have produced.

        Returns every identifier that did not cross, which the engine then
        says. Dropped here, they would be genes missing from the rest of
        the analysis with nothing to show why.
        """
        return _core.map_identifiers(
            entities.file,
            id_column,
            table.file,
            table_id_column,
            column,
            str(out),
        )

    return write(produce)


@step(tool="ligase", conversion="map_identifiers")
def map_ranked_identifiers(
    entities: RankedList,
    table: EntitySet,
    *,
    column: str,
    to: str,
    id_column: str = "id",
    table_id_column: str = "id",
) -> RankedList:
    """Rewrite a ranked list's identifiers, and say that its ranks moved.

    `map_identifiers` with one difference, and the difference is the reason
    this is a second step rather than an argument: the output is a
    `RankedList`, and a step's output kind is read off its return
    annotation once, when it is decorated. One function cannot produce
    both kinds.

    The mapping itself is the same work and keeps the same guarantees. Rows
    come out in the order they went in, so rank order survives; an
    identifier that does not cross is named rather than dropped quietly or
    carried into a space it is not in (D38).

    ### What is different, and why it needs saying

    An `EntitySet` that loses members is still an honest set of the ones
    that crossed. A ranking that loses rows is not simply a shorter
    ranking: rank is the row position (D41), so **every row below a
    dropped one moves up**. If the top-ranked identifier had no mapping,
    the second is now first.

    The output is a perfectly well-formed ranking of the entities that
    crossed, with ranks 1..n and nothing in the file to show that its rank
    3 was not the input's rank 3. So this step says it, with a
    `ranks-renumbered` note carrying what rank the output's first row held
    in the input — in the run report, in provenance, in a bundle and in the
    methods section, like every other note.

    The practical consequence that note is there to stop: **take a cut-off
    before mapping, not after.** The top ten of a mapped ranking is the top
    ten of what crossed, which is not the mapping of the top ten.

    Args:
        entities: The ranking to convert.
        table: The mapping, as an `EntitySet` whose own identifiers are the
            *source* side — in the same space as the ranking, so a table of
            the wrong space is refused before anything runs.
        column: The column of `table` holding the identifiers to convert
            *to*.
        to: The space the output is in, such as `Ensembl gene`. Declared
            rather than inferred, so what the output is does not depend on
            which identifiers happened to cross.
        id_column: The column of `entities` holding its identifiers.
        table_id_column: The column of `table` holding the source
            identifiers.

    Returns:
        A recipe, typed as the ranking the step produces.
    """

    def produce(out: Path) -> list[Noticed]:
        """Map, write, and work out how far the ranks moved."""
        noticed = _core.map_identifiers(
            entities.file,
            id_column,
            table.file,
            table_id_column,
            column,
            str(out),
        )
        first = _first_surviving_rank(
            entities.file, id_column, table.file, table_id_column, column, out
        )
        if first is not None:
            noticed.append(("ranks-renumbered", f"first={first}"))
        return noticed

    return write(produce)


def _first_surviving_rank(
    entities: str,
    id_column: str,
    table: str,
    table_id_column: str,
    column: str,
    out: Path,
) -> int | None:
    """What rank the output's first row held in the input, if any rank moved.

    `None` when nothing was lost, because then no rank moved and there is
    nothing to say -- a note for every conversion would teach people to
    skip the notes.

    Worked out from the three files rather than reported by the mapping,
    which names the identifiers that did not cross and not the positions
    they left behind. The names alone would not do: they are capped for a
    long list, so counting them would give a number that was too small,
    and a note with a wrong number in it is worse than no note.

    The output's identifiers are in the *target* space, so the leader is
    found by asking the table which of the input's identifiers it sent
    there. Exactly one can have: two input identifiers reaching one target
    is a collision, and a collision is left out of the output (D38).
    """
    before = _identifiers(Path(entities), id_column)
    after = _identifiers(out, id_column)
    if not after or len(after) == len(before):
        return None

    leader = _source_of(Path(table), table_id_column, column, after[0], before)
    # Unreachable while the mapping has succeeded: the output's first
    # identifier came out of this table, from a row whose own identifier was
    # in this list. Silence rather than a guessed rank if it ever is.
    return None if leader is None else before.index(leader) + 1


def _source_of(
    table: Path,
    id_column: str,
    column: str,
    target: str,
    candidates: list[str],
) -> str | None:
    """Which of `candidates` the mapping table sent to `target`."""
    wanted = set(candidates)
    rows = table.read_text().splitlines()
    if not rows:
        return None
    header = rows[0].split("\t")
    if id_column not in header or column not in header:
        return None
    at, to = header.index(id_column), header.index(column)

    for line in rows[1:]:
        fields = line.split("\t")
        if len(fields) <= max(at, to):
            continue
        if fields[to] == target and fields[at] in wanted:
            return fields[at]
    return None


def _identifiers(path: Path, id_column: str) -> list[str]:
    """The identifier column, in file order, from a TSV with a header."""
    lines = path.read_text().splitlines()
    if not lines:
        return []
    header = lines[0].split("\t")
    at = header.index(id_column) if id_column in header else 0
    return [
        fields[at]
        for fields in (line.split("\t") for line in lines[1:] if line.strip())
        if len(fields) > at
    ]
