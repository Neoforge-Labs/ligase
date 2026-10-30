//! Loading a knowledge graph, and reading off which sources built it.
//!
//! **The silent failure this catches.** A knowledge graph is the first kind
//! in Ligase that is *assembled* rather than measured. What it holds is a
//! function of which evidence sources were folded into it: add DisGeNET to a
//! graph that had Open Targets and a literature-mined set, and gene–disease
//! edges appear, neighbourhoods widen, and every score derived by
//! propagation or centrality moves. Nothing in the resulting file says which
//! sources went in. So two graphs that differ *only* in their source
//! composition diff, score and join against each other without complaint,
//! and the difference between them reads as a biological or methodological
//! result. A benchmark scored against a graph built from a different source
//! list measures the assembly rather than the method, and comes out
//! plausible. An edge present in one and absent in the other reads as
//! evidence of absence when it is the absence of a source.
//!
//! So the sources are a dimension — `source_set` — and this is the loader
//! that fills it in. That is what admits the kind under
//! [D35](https://ligase.neoforgelabs.tech/project/decisions/): it brings an
//! axis no other kind brings, whose mismatch is silent, plausible, and
//! attributable to biology by a reader who cannot see it.
//!
//! # The sources are read, not asked for
//!
//! They are on the edges, in the per-edge provenance a graph carries, so
//! refusing to read them would be the engine asking for something it is
//! already holding (D44). A declaration still wins where there is one, for
//! the reason a named list's namespace declaration wins: a declaration is a
//! user telling the engine something it cannot read, and a graph's own
//! `sources` may name a mirror, a shard or an internal pipeline stage rather
//! than the source a reader would recognise.
//!
//! An inference that comes back empty is a **gap**, never an empty set. Two
//! graphs that each recorded no sources would otherwise *agree* on the axis,
//! which is exactly the silence the dimension exists to break — the rule
//! [`NameSet`](crate::dim::NameSet) states and [`super::organism`] already
//! holds for an empty organism declaration.
//!
//! # What this loader does not do
//!
//! Read anybody else's graph format. Open Targets' Parquet, Monarch's dumps,
//! and Neo4j and RDF exports are all outside it: what is read is the
//! node/edge JSON described on
//! [the guide](https://ligase.neoforgelabs.tech/guide/knowledge-graphs/).
//! Defining a shape is not what earns a kind its place, and this one is the
//! cheapest envelope that can carry per-edge provenance.

use std::path::Path;

use super::graph::GraphReader;
use super::{Declared, LoadError, open};
use crate::dim::{Dim, Dimensions, IdentifierNamespace, Inference, KnownNamespace, Source};
use crate::types::{DataKind, Dataset};

/// Why the organism is unknown: a graph does not record it.
const NO_ORGANISM: &str = "a knowledge graph does not record which organisms its entities \
                           belong to";

/// Why the subject is unknown.
const NO_SUBJECT: &str = "a knowledge graph does not record what it is a graph of";

/// Why the release is unknown.
const NO_RELEASE: &str = "a knowledge graph does not record which release its sources were \
                          drawn from";

/// Why the source set is unknown when the graph named none.
const NO_SOURCES: &str = "neither the graph nor any of its nodes and edges names a source -- \
                          declare source_set";

/// Why the namespace is unknown when the node ids did not settle it.
///
/// Deliberately different advice from a gene list's. A graph of genes and
/// diseases names its entities in two spaces by design, so "the ids do not
/// all match one pattern" is the *normal* case here rather than a sign of
/// something wrong, and the sentence says so instead of implying a fault.
const NOT_INFERRED: &str = "not declared, and the node ids do not all match one namespace's \
                            pattern -- which is ordinary for a graph holding more than one \
                            kind of entity; declare identifier_namespace";

/// Why the namespace is unknown when every node id is a bare symbol.
///
/// Separate from [`NOT_INFERRED`] for the same reason a gene list's is
/// (D56): the ids are readable and all fit one shape, and that shape has no
/// marker, so the engine has nothing to read the space off. Worth its own
/// sentence here because a graph whose ids are *all* symbol-shaped is a
/// graph of one kind of entity, which is the case the sentence above
/// explicitly calls unusual.
const INDISTINGUISHABLE: &str = "not declared, and the node ids are bare gene-symbol shapes \
                                 -- symbols and several synonym spaces are indistinguishable \
                                 by pattern, so declare identifier_namespace";

/// Load a knowledge graph from JSON.
///
/// Nodes and edges are read, their `sources` lists unioned into the
/// `source_set` dimension, and the identifier namespace read off the node
/// ids where they all agree. Everything else a node or an edge says is
/// carried without being read.
///
/// # Errors
///
/// [`LoadError::Io`] if the file cannot be read, and [`LoadError::Format`]
/// if it is not JSON, has no `nodes` or `edges` array, or holds a node with
/// no `id` or an edge with no `source`, `target` or `type`.
pub fn knowledge_graph(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    let reader = open(path)?;
    let graph = GraphReader::read(reader).map_err(|detail| LoadError::format(path, detail))?;

    let dimensions = Dimensions {
        organism: super::organism(declared, NO_ORGANISM),
        identifier_namespace: namespace(path, declared, &graph),
        subject: super::subject(declared, NO_SUBJECT),
        release: super::release(declared, NO_RELEASE),
        source_set: super::source_set(
            declared,
            graph.every_source(),
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "the sources named on its nodes and edges",
            },
            NO_SOURCES,
        ),
        ..Dimensions::default()
    };

    Ok(Dataset::new(
        DataKind::KnowledgeGraph,
        Some(path.to_path_buf()),
        dimensions,
    ))
}

/// The namespace dimension: declared if stated, else read off the node ids.
///
/// The same inference a named list gets, over the ids an edge can point at.
/// A node's other `ids` are deliberately not consulted: they are the names
/// the entity *also* has, in whatever spaces somebody recorded, and feeding
/// them to inference would guarantee a mixed set and so guarantee a decline.
fn namespace(path: &Path, declared: &Declared, graph: &GraphReader) -> Dim<IdentifierNamespace> {
    if let Some(label) = declared
        .identifier_namespace
        .as_deref()
        .map(IdentifierNamespace::new)
        && !label.is_empty()
    {
        return Dim::known(label, Source::Declared);
    }

    match KnownNamespace::infer(graph.nodes.iter().map(|node| node.id.as_str())) {
        Inference::Settled(found) => Dim::known(
            IdentifierNamespace::new(found.name()),
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "the node ids' own pattern",
            },
        ),
        Inference::Indistinguishable(_) => Dim::unknown(INDISTINGUISHABLE),
        Inference::Unsettled => Dim::unknown(NOT_INFERRED),
    }
}
