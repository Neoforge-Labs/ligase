//! Reading a knowledge graph's nodes and edges out of JSON.
//!
//! Shared by [`super::knowledge_graph`] and
//! [`canon::knowledge_graph`](crate::canon), the way [`super::table`] is
//! shared by the named-list loaders and their canonicalisers. Deciding what
//! a node is, once, is the part where disagreeing would be a bug: a loader
//! that read a node the canonicaliser did not would report dimensions for a
//! graph whose hash was computed over something else.
//!
//! # Strict, like every reader here
//!
//! A node needs a string `id`; an edge needs a string `source`, `target` and
//! `type`. Anything else is not the thing it claims to be, and saying so is
//! what makes the fallback in [`canon::hash`](crate::canon::hash) correct:
//! a file that will not parse hashes as its own bytes rather than as the
//! canonical form of something it is not. A reader that took what it
//! understood and skipped the rest would be the dangerous kind of lenient,
//! because then two files differing only in the skipped part would agree.
//!
//! # What is pulled out, and what is carried
//!
//! `id`, `type`, and the `ids` and `sources` lists are pulled out, because
//! the engine has something to say about each: the ids and the sources are
//! sets, so they are sorted before they are hashed and their order is not
//! part of what the graph says. Every other key is *carried* — kept in
//! `rest`, hashed as it stands, and never read. What a graph says about its
//! own nodes is the user's business, exactly as a gene list's attribute
//! columns are.

use std::io::Read;

use serde_json::{Map, Value};

/// One node, with the parts the engine has something to say about pulled out.
pub(crate) struct GraphNode {
    /// The identifier an edge points at.
    pub(crate) id: String,
    /// Other names for the same entity, as a set.
    pub(crate) ids: Vec<String>,
    /// The sources that contributed this node, as a set.
    pub(crate) sources: Vec<String>,
    /// Every other key, carried without being read.
    pub(crate) rest: Map<String, Value>,
}

/// One edge, with the same treatment.
pub(crate) struct GraphEdge {
    /// The node this edge starts at.
    pub(crate) from: String,
    /// The node it ends at.
    pub(crate) to: String,
    /// What kind of edge it is.
    pub(crate) relation: String,
    /// The sources that contributed this edge: the per-edge provenance the
    /// `source_set` dimension is read from.
    pub(crate) sources: Vec<String>,
    /// Every other key, carried without being read.
    pub(crate) rest: Map<String, Value>,
}

/// A graph as this reader understands it.
pub(crate) struct GraphReader {
    /// The nodes, in the order the file lists them. The order is not part of
    /// what the graph says; it is kept only so a message could echo it.
    pub(crate) nodes: Vec<GraphNode>,
    /// The edges, likewise.
    pub(crate) edges: Vec<GraphEdge>,
    /// The sources the graph names once, at the top level, for a graph that
    /// records its assembly in one place rather than on every edge.
    pub(crate) sources: Vec<String>,
    /// Every other top-level key, carried without being read.
    pub(crate) rest: Map<String, Value>,
}

impl GraphReader {
    /// Read a graph, or say why this is not one.
    pub(crate) fn read(mut reader: impl Read) -> Result<Self, String> {
        // Read whole and parse whole. A streaming parse would buy nothing:
        // the keys have to be sorted at every depth to canonicalise, which
        // needs the document anyway. The honest consequence is that a graph
        // larger than memory is not readable, and nothing pretends
        // otherwise.
        let mut body = Vec::new();
        reader
            .read_to_end(&mut body)
            .map_err(|error| error.to_string())?;
        let parsed: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;

        let Value::Object(mut fields) = parsed else {
            return Err("a knowledge graph is a JSON object with `nodes` and `edges`".to_owned());
        };

        let nodes = fields
            .remove("nodes")
            .ok_or_else(|| "no `nodes` array: a knowledge graph needs one".to_owned())?;
        let edges = fields
            .remove("edges")
            .ok_or_else(|| "no `edges` array: a knowledge graph needs one".to_owned())?;
        let sources = match fields.remove("sources") {
            Some(value) => names(&value, "the graph's `sources`")?,
            None => Vec::new(),
        };

        Ok(Self {
            nodes: array("nodes", nodes)?
                .into_iter()
                .map(node)
                .collect::<Result<_, _>>()?,
            edges: array("edges", edges)?
                .into_iter()
                .map(edge)
                .collect::<Result<_, _>>()?,
            sources,
            rest: fields,
        })
    }

    /// Every source the graph names, anywhere, in the order first seen.
    ///
    /// The union of the top level's, the nodes' and the edges', because a
    /// graph assembled by a pipeline says what went into it in whichever of
    /// those places its assembler chose. Deduplication is the dimension's
    /// business, not this reader's: [`SourceSet`](crate::dim::SourceSet)
    /// decides what two spellings mean, and it compares them exactly.
    pub(crate) fn every_source(&self) -> Vec<&str> {
        self.sources
            .iter()
            .chain(self.nodes.iter().flat_map(|node| node.sources.iter()))
            .chain(self.edges.iter().flat_map(|edge| edge.sources.iter()))
            .map(String::as_str)
            .collect()
    }
}

/// The array a key holds, or why it is not one.
fn array(key: &str, value: Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err(format!("`{key}` is not an array")),
    }
}

/// The object an array's item holds, or why it is not one.
fn object(key: &str, value: Value) -> Result<Map<String, Value>, String> {
    match value {
        Value::Object(fields) => Ok(fields),
        _ => Err(format!("an entry of `{key}` is not an object")),
    }
}

/// A required string field, taken out of the object.
fn string(fields: &mut Map<String, Value>, key: &str, what: &str) -> Result<String, String> {
    match fields.remove(key) {
        Some(Value::String(found)) if !found.trim().is_empty() => Ok(found.trim().to_owned()),
        Some(_) => Err(format!("{what} has a `{key}` that is not a name")),
        None => Err(format!("{what} has no `{key}`")),
    }
}

/// A list of names, which must be strings if it is there at all.
fn names(value: &Value, what: &str) -> Result<Vec<String>, String> {
    let Value::Array(items) = value else {
        return Err(format!("{what} is not an array"));
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(found) => Ok(found.trim().to_owned()),
            _ => Err(format!("{what} holds something that is not a name")),
        })
        .collect()
}

/// An optional list of names, taken out of the object.
fn optional_names(
    fields: &mut Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<Vec<String>, String> {
    match fields.remove(key) {
        Some(value) => names(&value, &format!("{what}'s `{key}`")),
        None => Ok(Vec::new()),
    }
}

fn node(value: Value) -> Result<GraphNode, String> {
    let mut fields = object("nodes", value)?;
    let id = string(&mut fields, "id", "a node")?;
    let ids = optional_names(&mut fields, "ids", "a node")?;
    let sources = optional_names(&mut fields, "sources", "a node")?;
    Ok(GraphNode {
        id,
        ids,
        sources,
        rest: fields,
    })
}

fn edge(value: Value) -> Result<GraphEdge, String> {
    let mut fields = object("edges", value)?;
    let from = string(&mut fields, "source", "an edge")?;
    let to = string(&mut fields, "target", "an edge")?;
    let relation = string(&mut fields, "type", "an edge")?;
    let sources = optional_names(&mut fields, "sources", "an edge")?;
    Ok(GraphEdge {
        from,
        to,
        relation,
        sources,
        rest: fields,
    })
}
