//! The "stdio → running app" bridge for AI clients that can only launch processes (stdio). Each input line goes as a
//! POST to the app's `HttpEndpoint`, the reply comes back as an output line. If the app is not running, every request
//! with an `id` gets a clear error instead of silence.

use std::io::{self, BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;

use crate::envelope::{SERVER_ERROR, failure};
use crate::http::PATH;

const TIMEOUT: Duration = Duration::from_secs(120);

/// One POST; returns (status, body).
fn post(port: u16, body: &str) -> io::Result<(u16, String)> {
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(3))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    write!(
        stream,
        "POST {PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let (head, text) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let status = head.split_whitespace().nth(1).and_then(|code| code.parse().ok()).unwrap_or(0);
    Ok((status, text.to_string()))
}

fn error_reply(id: &Value, text: &str) -> String {
    failure(id, SERVER_ERROR, text).to_string()
}

/// Forwards the lines of `input` to the app on `port` and writes the replies to `output` until the input ends.
pub fn relay(port: u16, input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let id = serde_json::from_str::<Value>(&line).ok().and_then(|message| message.get("id").cloned());
        let reply = match post(port, &line) {
            Ok((200, body)) => Some(body.trim().to_string()),
            Ok((202, _)) => None,
            Ok((status, body)) => id.as_ref().map(|id| error_reply(id, &format!("the app replied {status}: {}", body.trim()))),
            Err(error) => id.as_ref().map(|id| {
                error_reply(
                    id,
                    &format!("the DAW app does not answer on port {port} ({error}); start it and check that the AI server is on"),
                )
            }),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}
