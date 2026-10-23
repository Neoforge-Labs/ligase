//! What is asked, and which record that makes it.
//!
//! A pinned remote is identified by its locator, because an accession names
//! a thing. A recorded query cannot be: twenty paginated calls share a
//! locator's host and path and differ only in a parameter, and a step that
//! makes them in a different order next time must still find the same
//! twenty records. So a record's identity is the **canonical request** —
//! method, URL with its parameters sorted, request body, and nothing
//! volatile (D39).
//!
//! # What is sent and what is hashed are two things
//!
//! [`Request::url`] is the URL as the user wrote it, and it is what goes on
//! the wire. [`CanonicalRequest::url`] is that URL with its query parameters
//! sorted, and it is what is hashed.
//!
//! They are deliberately two fields rather than one normalised one. A server
//! may care about the order of its parameters, and the user wrote what they
//! wrote; reordering the request to match the hash would be the engine
//! quietly asking a different question. Sorting is for deciding *which
//! record this is*, never for deciding *what to ask*.
//!
//! # A token is how you are let in, not what you asked for
//!
//! [`Auth`] is on the request, because it has to be sent, and it is on
//! nothing else: not on [`CanonicalRequest`], which is why two runs of one
//! pipeline — one authenticated, one not — replay each other's records, and
//! not on [`super::fetch::Fetched`] or on anything derived from it, which is
//! how "a bundle never carries a secret" is structural rather than a filter.
//!
//! The one door that leaves open is a message that formats the request, so
//! [`Auth`]'s own [`fmt::Debug`] redacts the token. A secret that is
//! unrepresentable everywhere it could be stored and unprintable where it
//! cannot be is as far as a type system can carry the guarantee.

use std::fmt;

use crate::hash::ContentHash;

/// Separates a request hash from every other BLAKE3 hash in the engine.
const DOMAIN: &str = "ligase.request";

/// Which HTTP method a request uses.
///
/// Two, and only one of them can be issued: the transport does `GET` and
/// nothing else in v0.2. `Post` exists because the method is part of a
/// record's identity, and hashing it now means a later `POST` — Open
/// Targets' GraphQL is one — does not change the identity of a record
/// written today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// Ask for a resource. Everything the transport can do.
    Get,
    /// Ask with a body. Part of a record's identity, not yet sendable.
    Post,
}

impl Method {
    /// The method's name, as HTTP spells it and as the record stores it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }

    /// The method this name stands for, for reading back a stored record.
    ///
    /// `None` for a name this version does not know, so a record written by
    /// a later one is skipped rather than half-understood.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            _ => None,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How a request is authenticated.
///
/// Only a bearer token, on the closed-set reasoning the four locator schemes
/// rest on: one scheme the engine knows how to keep out of everything it
/// writes is a promise it can keep, where an open set of header names would
/// be a promise it could not.
#[derive(Clone, PartialEq, Eq)]
pub struct Auth {
    token: String,
}

impl Auth {
    /// A bearer token.
    ///
    /// The value, not the name of the variable it came from: reading the
    /// environment happens where the request is built, so that this type
    /// never needs to know where a secret lives.
    #[must_use]
    pub fn bearer(token: &str) -> Self {
        Self {
            token: token.to_owned(),
        }
    }

    /// The header this sends, as a name and a value.
    ///
    /// The only place the token leaves this type, and the caller that uses
    /// it hands it straight to the client.
    #[must_use]
    pub fn header(&self) -> (&'static str, String) {
        ("Authorization", format!("Bearer {}", self.token))
    }
}

impl fmt::Debug for Auth {
    /// Says that there is a token and not what it is.
    ///
    /// Hand-written rather than derived because a derived one would print
    /// the token into any message that formats a request, which is the one
    /// way a secret could still escape a type that is stored nowhere.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Auth(<redacted>)")
    }
}

/// A request about to be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Which method, which in v0.2 the transport requires to be `Get`.
    pub method: Method,
    /// The URL **as written**, which is what is sent. See the module
    /// documentation for why this is not the sorted one.
    pub url: String,
    /// The request body, empty for a `GET`.
    pub body: Vec<u8>,
    /// How to authenticate, where that is needed. Never stored.
    pub auth: Option<Auth>,
}

impl Request {
    /// A plain `GET`, which is what a pinned remote and almost every query
    /// is.
    #[must_use]
    pub fn get(url: &str) -> Self {
        Self {
            method: Method::Get,
            url: url.to_owned(),
            body: Vec::new(),
            auth: None,
        }
    }

    /// A `POST` with a body.
    ///
    /// Buildable and hashable; not sendable, because the transport does
    /// `GET` only. See [`Method`].
    #[must_use]
    pub fn post(url: &str, body: &[u8]) -> Self {
        Self {
            method: Method::Post,
            url: url.to_owned(),
            body: body.to_vec(),
            auth: None,
        }
    }

    /// The same request, authenticated.
    #[must_use]
    pub fn with_auth(mut self, auth: Option<Auth>) -> Self {
        self.auth = auth;
        self
    }
}

/// A request reduced to what identifies it.
///
/// Built from a [`Request`] by sorting its query parameters and forgetting
/// its authentication. Its [`hash`](Self::hash) is a record's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalRequest {
    method: Method,
    url: String,
    body: Vec<u8>,
}

impl CanonicalRequest {
    /// Reduce a request to what identifies it.
    #[must_use]
    pub fn of(request: &Request) -> Self {
        Self {
            method: request.method,
            url: canonical_url(&request.url),
            body: request.body.clone(),
        }
    }

    /// Which method this identifies.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// The URL with its parameters sorted, which is what is hashed and is
    /// not what is sent.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The request body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The hash that is this record's name.
    ///
    /// Domain-separated, so a request hash can never equal a file's content
    /// hash, and every field length-prefixed, so a URL ending in `POST`
    /// cannot be mistaken for a method.
    #[must_use]
    pub fn hash(&self) -> ContentHash {
        let mut preimage = Vec::with_capacity(self.url.len() + self.body.len() + 32);
        for field in [
            DOMAIN.as_bytes(),
            self.method.name().as_bytes(),
            self.url.as_bytes(),
            self.body.as_slice(),
        ] {
            preimage.extend_from_slice(&(field.len() as u64).to_le_bytes());
            preimage.extend_from_slice(field);
        }
        ContentHash::of_bytes(&preimage)
    }
}

impl fmt::Display for CanonicalRequest {
    /// `GET https://…`, which is how a drift report names a request.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method, self.url)
    }
}

/// A URL with its query parameters sorted and its fragment dropped.
///
/// The fragment goes because it is never sent: it is the client's business,
/// so two URLs differing only in one are one request. The parameters are
/// sorted as whole `name=value` strings rather than by name, so a parameter
/// repeated with two values keeps both — `?id=a&id=b` asks for two things
/// and is not `?id=a`.
///
/// The host's case is left alone. Two spellings of one host would be two
/// records, which costs a fetch and cannot produce a wrong answer; folding
/// them would be the engine deciding that two URLs the user wrote
/// differently are the same, which is the kind of guess this project does
/// not make.
fn canonical_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let Some((base, query)) = without_fragment.split_once('?') else {
        return without_fragment.to_owned();
    };
    if query.is_empty() {
        return base.to_owned();
    }
    let mut parameters: Vec<&str> = query.split('&').filter(|part| !part.is_empty()).collect();
    parameters.sort_unstable();
    if parameters.is_empty() {
        return base.to_owned();
    }
    format!("{base}?{}", parameters.join("&"))
}
