//! A local HTTP server for the remote tests, on `std::net` and nothing else.
//!
//! Three of M10's acceptance clauses turn on behaviour a canned HTTP mock
//! will not give you: *the server is down* has to be a real closed socket, not
//! a stubbed error; *an auth header leaves no trace* has to be checked against
//! what the server actually received; and *two paginated calls replay in
//! either order* has to be checked against the order they were issued in. So
//! the server records every request and can be shut down, and the whole thing
//! is a hundred lines of `TcpListener` rather than a dependency.
//!
//! It speaks only as much HTTP/1.1 as the engine's client asks of it:
//! request line, headers, an optional body by `Content-Length`, and a
//! response with `Content-Length` and `Connection: close`.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One response the server will give.
#[derive(Clone, Debug)]
pub struct Reply {
    /// The status code.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
    /// Extra response headers, beyond `Content-Length` and `Connection`.
    pub headers: Vec<(String, String)>,
}

impl Reply {
    /// A 200 with this body.
    pub fn ok(body: impl AsRef<[u8]>) -> Self {
        Self {
            status: 200,
            body: body.as_ref().to_vec(),
            headers: Vec::new(),
        }
    }

    /// A status with an empty body.
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            headers: Vec::new(),
        }
    }

    /// A 302 to somewhere else.
    pub fn redirect(to: &str) -> Self {
        Self {
            status: 302,
            body: Vec::new(),
            headers: vec![("Location".to_owned(), to.to_owned())],
        }
    }

    /// With one more response header.
    #[must_use]
    pub fn and_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

/// What the server was asked for.
#[derive(Clone, Debug)]
pub struct Received {
    /// `GET`, `POST`, …
    pub method: String,
    /// The request target, path and query as written.
    pub target: String,
    /// Every header, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The request body, where there was one.
    pub body: Vec<u8>,
}

impl Received {
    /// One header's value, if it was sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(had, _)| *had == wanted)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Default)]
struct State {
    /// Path (without query) to the replies for it. The last reply repeats,
    /// so a one-reply route answers every request the same way and a
    /// several-reply route can fail twice and then succeed.
    routes: HashMap<String, Vec<Reply>>,
    served: HashMap<String, usize>,
    received: Vec<Received>,
}

/// A local HTTP server that remembers what it was asked.
pub struct MockServer {
    address: SocketAddr,
    state: Arc<Mutex<State>>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    /// Start a server on a port the operating system picks.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let stopping = Arc::new(AtomicBool::new(false));

        let thread = {
            let state = Arc::clone(&state);
            let stopping = Arc::clone(&stopping);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    // Serialised on purpose: the request log is an ordered
                    // record, and these tests care about the order.
                    serve(&stream, &state);
                }
            })
        };

        Self {
            address,
            state,
            stopping,
            thread: Some(thread),
        }
    }

    /// The server's base URL, with no trailing slash.
    pub fn base(&self) -> String {
        format!("http://{}", self.address)
    }

    /// A URL on this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base())
    }

    /// Answer this path with this body, every time.
    pub fn on(&self, path: &str, body: impl AsRef<[u8]>) {
        self.replies(path, vec![Reply::ok(body)]);
    }

    /// Answer this path with these replies in turn; the last one repeats.
    pub fn replies(&self, path: &str, replies: Vec<Reply>) {
        let mut state = self.state.lock().unwrap();
        state.routes.insert(path.to_owned(), replies);
        state.served.remove(path);
    }

    /// Every request the server has had, in the order it had them.
    pub fn received(&self) -> Vec<Received> {
        self.state.lock().unwrap().received.clone()
    }

    /// How many requests have arrived for one path.
    pub fn hits(&self, path: &str) -> usize {
        // Counted by whichever spelling the caller used: a bare path counts
        // every query against it, and a path with a query string counts
        // only that one, which is what a pagination test needs.
        self.received()
            .iter()
            .filter(|request| {
                if path.contains('?') {
                    request.target == path
                } else {
                    request.target.split('?').next() == Some(path)
                }
            })
            .count()
    }

    /// Stop serving and close the port.
    ///
    /// What "runs again with the server down" means: after this, a fetch to
    /// this address fails to connect, the way it would against a machine
    /// that has gone away.
    pub fn shutdown(&mut self) {
        if self.thread.is_none() {
            return;
        }
        self.stopping.store(true, Ordering::SeqCst);
        // Unblock the accept loop so the thread notices the flag.
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn serve(stream: &TcpStream, state: &Arc<Mutex<State>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();

    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if name == "content-length" {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    let mut body = vec![0u8; length];
    if length > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    let path = target.split('?').next().unwrap_or(&target).to_owned();
    let reply = {
        let mut state = state.lock().unwrap();
        // A route registered with a query string matches the whole target;
        // one registered without it matches the path whatever the query.
        // Paginated calls differ only in a parameter, so a mock that could
        // not tell `?page=1` from `?page=2` could not test them.
        let route = if state.routes.contains_key(&target) {
            target.clone()
        } else {
            path.clone()
        };
        state.received.push(Received {
            method,
            target,
            headers,
            body,
        });
        let served = state.served.entry(route.clone()).or_default();
        let at = *served;
        *served += 1;
        state.routes.get(&route).map_or_else(
            || Reply::status(404),
            |replies| replies[at.min(replies.len() - 1)].clone(),
        )
    };

    let mut out = Vec::new();
    let reason = match reply.status {
        200 => "OK",
        302 => "Found",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    out.extend_from_slice(format!("HTTP/1.1 {} {reason}\r\n", reply.status).as_bytes());
    for (name, value) in &reply.headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n", reply.body.len()).as_bytes());
    out.extend_from_slice(b"Connection: close\r\n\r\n");
    out.extend_from_slice(&reply.body);

    let mut stream = stream;
    let _ = stream.write_all(&out);
    let _ = stream.flush();
}
