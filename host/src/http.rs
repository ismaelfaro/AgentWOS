//! A very small HTTP/1.1 server for the one job of answering outside agents, with
//! every limit chosen to refuse rather than to be generous:
//!
//! - it listens on the loopback interface only, and never on another address;
//! - nothing is answered without the bearer token, compared in constant time;
//! - headers, body and time are capped; a chunked body is read with the same cap and
//!   no trailers;
//! - one connection at a time, closed after its answer.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use anyhow::{bail, Result};

const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Self { status, body: value.to_string().into_bytes() }
    }

    pub fn empty(status: u16) -> Self {
        Self { status, body: Vec::new() }
    }
}

/// Binds the loopback address. Any other address is an error.
pub fn listen(addr: &str) -> Result<TcpListener> {
    let addr: SocketAddr = addr.parse().map_err(|_| anyhow::anyhow!("`{addr}` is not an address like 127.0.0.1:8820"))?;
    if !addr.ip().is_loopback() {
        bail!("{addr} is not a loopback address. This build serves on 127.0.0.1 only: put a proxy you control in front of it to reach it from elsewhere");
    }
    Ok(TcpListener::bind(addr)?)
}

pub fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

fn read_request(stream: &mut TcpStream) -> std::result::Result<Request, (u16, &'static str)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEADERS {
            return Err((431, "headers too large"));
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err((400, "incomplete request")),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8(buf[..end].to_vec()).map_err(|_| (400, "headers are not text"))?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, path) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
    let mut headers = HashMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or((400, "bad header"))?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let leftover = buf[end + 4..].to_vec();
    if let Some(encoding) = headers.get("transfer-encoding") {
        if !encoding.eq_ignore_ascii_case("chunked") || headers.contains_key("content-length") {
            return Err((400, "unsupported transfer encoding"));
        }
        let body = read_chunked(stream, leftover)?;
        return Ok(Request { method, path, headers, body });
    }
    let length: usize = headers.get("content-length").map_or(Ok(0), |v| v.parse()).map_err(|_| (400, "bad Content-Length"))?;
    if length > MAX_BODY {
        return Err((413, "body too large"));
    }
    let mut body = buf[end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err((400, "incomplete body")),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    Ok(Request { method, path, headers, body })
}

/// Next line (up to CRLF) from `pending`, reading more when it is not there yet.
fn take_line(stream: &mut TcpStream, pending: &mut Vec<u8>) -> std::result::Result<String, (u16, &'static str)> {
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(i) = pending.windows(2).position(|w| w == b"\r\n") {
            let line = String::from_utf8(pending[..i].to_vec()).map_err(|_| (400, "bad chunk header"))?;
            pending.drain(..i + 2);
            return Ok(line);
        }
        if pending.len() > 64 {
            return Err((400, "bad chunk header"));
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err((400, "incomplete body")),
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
        }
    }
}

fn read_chunked(stream: &mut TcpStream, mut pending: Vec<u8>) -> std::result::Result<Vec<u8>, (u16, &'static str)> {
    let mut chunk = [0u8; 2048];
    let mut body = Vec::new();
    loop {
        let line = take_line(stream, &mut pending)?;
        let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16).map_err(|_| (400, "bad chunk size"))?;
        if size == 0 {
            // No trailers: the next line must be empty.
            return if take_line(stream, &mut pending)?.is_empty() { Ok(body) } else { Err((400, "trailers are not accepted")) };
        }
        if body.len() + size > MAX_BODY {
            return Err((413, "body too large"));
        }
        while pending.len() < size + 2 {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => return Err((400, "incomplete body")),
                Ok(n) => pending.extend_from_slice(&chunk[..n]),
            }
        }
        if &pending[size..size + 2] != b"\r\n" {
            return Err((400, "bad chunk"));
        }
        body.extend_from_slice(&pending[..size]);
        pending.drain(..size + 2);
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    }
}

fn write_response(stream: &mut TcpStream, response: &Response) {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        response.status,
        reason(response.status),
        response.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

/// Serves requests one by one. `token` is required on every request.
pub fn serve(listener: TcpListener, token: &str, handler: &mut dyn FnMut(Request) -> Response) -> Result<()> {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let response = match read_request(&mut stream) {
            Err((status, why)) => Response::json(status, &serde_json::json!({ "error": why })),
            Ok(request) => {
                let given = request.headers.get("authorization").and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
                if !same_secret(given, token) {
                    Response::json(401, &serde_json::json!({ "error": "unauthorized" }))
                } else {
                    handler(request)
                }
            }
        };
        write_response(&mut stream, &response);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_addresses_are_listened_on() {
        assert!(listen("0.0.0.0:0").is_err());
        assert!(listen("192.168.1.5:0").is_err());
        assert!(listen("127.0.0.1:0").is_ok());
    }

    #[test]
    fn secrets_compare_whole() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
        assert!(!same_secret("", "x"));
    }
}
