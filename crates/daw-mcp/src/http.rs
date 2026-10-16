//! MCP over HTTP inside the running app (the "Streamable HTTP" transport, single JSON replies): the AI client connects
//! to the same project that is open in the window, and its edits show up on screen right away.
//!
//! Listens ONLY on `127.0.0.1`. A foreign website in a browser must not be able to control the program, so a request is
//! rejected if it has an `Origin` header (browsers always send it, regular clients never do), a foreign `Host` (DNS
//! rebinding protection) or a non-JSON body. Requests are served one at a time on one thread, so project edits never
//! race each other.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use daw_contracts::LogEntry;

use crate::McpServer;

const DEFAULT_PORT: u16 = 47_321;
/// The path the server answers on.
pub(crate) const PATH: &str = "/mcp";

const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 8 * 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The server port: the `DAW_MCP_PORT` environment variable or the default port (the bridge can still override it with `--port`).
pub fn port_from_env() -> u16 {
    std::env::var("DAW_MCP_PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(DEFAULT_PORT)
}

pub struct HttpEndpoint {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HttpEndpoint {
    /// Starts listening on `127.0.0.1:port` (0 means any free port). A busy port is an error: then the server just does not start.
    pub fn start(server: Arc<McpServer>, port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new().name("daw-mcp-http".into()).spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                // A connection dropped mid-request is the client's business: the server goes on, but leaves a trace in the log.
                if let Err(error) = stream.and_then(|stream| serve(&server, stream, port)) {
                    server.log().record(LogEntry::warn("ai", format!("an AI server request was not served: {error}")));
                }
            }
        })?;
        Ok(Self { port, stop, thread: Some(thread) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for HttpEndpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake `accept` up: without a new connection the thread would keep waiting.
        let _ = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)));
        if let Some(thread) = self.thread.take() {
            // A panic of the server thread is already in the log (the shared panic hook); nothing to add on close.
            let _ = thread.join();
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed before the end of the headers"));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
        if head.len() > MAX_HEADERS {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "headers are too long"));
        }
    }
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let (method, path) = (first.next().unwrap_or_default().to_string(), first.next().unwrap_or_default().to_string());
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    let length = headers.iter().find(|(name, _)| name == "content-length").and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
    if length > MAX_BODY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "request body is too large"));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, headers, body })
}

/// An HTTP reply: status, extra headers (each ending with `\r\n`) and body.
struct Reply {
    status: &'static str,
    extra: &'static str,
    body: String,
}

impl Reply {
    fn error(status: &'static str, text: &str) -> Self {
        Self { status, extra: "", body: serde_json::json!({ "error": text }).to_string() }
    }
}

/// Answers the client: protection rules first, then the protocol.
fn answer(server: &McpServer, request: &Request, port: u16) -> Reply {
    let error = Reply::error;
    let host_ok = request.header("host").is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    if !host_ok {
        return error("403 Forbidden", "wrong Host: the server only answers on 127.0.0.1 and localhost");
    }
    if request.header("origin").is_some() {
        return error("403 Forbidden", "browser requests (with an Origin header) are forbidden");
    }
    if request.path != PATH {
        return error("404 Not Found", &format!("only {PATH} exists"));
    }
    if request.method != "POST" {
        return Reply { extra: "Allow: POST\r\n", ..error("405 Method Not Allowed", "only POST is supported") };
    }
    if !request.header("content-type").is_some_and(|t| t.starts_with("application/json")) {
        return error("415 Unsupported Media Type", "Content-Type: application/json is required");
    }
    let Ok(text) = std::str::from_utf8(&request.body) else {
        return error("400 Bad Request", "the request body is not UTF-8");
    };
    match server.handle_line(text) {
        Some(body) => Reply { status: "200 OK", extra: "", body },
        // A notification: the reply has no body.
        None => Reply { status: "202 Accepted", extra: "", body: String::new() },
    }
}

fn serve(server: &McpServer, mut stream: TcpStream, port: u16) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let reply = match read_request(&mut stream) {
        Ok(request) => answer(server, &request, port),
        Err(error) => Reply::error("400 Bad Request", &error.to_string()),
    };
    let Reply { status, extra, body } = reply;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
