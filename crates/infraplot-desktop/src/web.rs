//! "Open in web": uploads the diagram to an infra-plot server (`PUT /api/diagrams/<id>`) so
//! the web editor can open it at `/d/<id>` (with the 3D view and the SVG/PNG exports).
//!
//! Plain `http://` is spoken directly over a socket; `https://` goes through `curl`, to keep
//! a TLS stack out of the app.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::Duration;

use infraplot_model::Diagram;

/// Where `infraplot-server` listens by default.
pub const DEFAULT_URL: &str = "http://127.0.0.1:31080";

/// A server id (`[a-z0-9_-]{1,64}`, starting with a letter or digit) from a file name or
/// title.
#[must_use]
pub fn diagram_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            id.push(c);
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id: String = id.trim_end_matches('-').chars().take(64).collect();
    let id = id.trim_end_matches('-');
    if id.is_empty() {
        "diagram".into()
    } else {
        id.to_owned()
    }
}

/// `base` without a trailing slash, with `http://` added when there is no scheme.
#[must_use]
pub fn normalize_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.contains("://") {
        base.to_owned()
    } else {
        format!("http://{base}")
    }
}

/// Uploads `doc` as `id` and returns the editor URL. Blocking.
pub fn publish(base: &str, id: &str, doc: &Diagram) -> Result<String, String> {
    let base = normalize_base(base);
    let body = doc.to_json();
    let url = format!("{base}/api/diagrams/{id}");
    let (status, reply) = if let Some(rest) = base.strip_prefix("http://") {
        put_http(rest, &format!("/api/diagrams/{id}"), &body)?
    } else if base.starts_with("https://") {
        put_curl(&url, &body)?
    } else {
        return Err(format!("unsupported URL {base}: use http:// or https://"));
    };
    if (200..300).contains(&status) {
        Ok(format!("{base}/d/{id}"))
    } else {
        let reply = reply.trim();
        let detail: String = reply.chars().take(300).collect();
        Err(format!("{url} answered {status}: {detail}"))
    }
}

fn put_http(rest: &str, path: &str, body: &str) -> Result<(u16, String), String> {
    let (host, prefix) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    let path = if prefix.is_empty() {
        path.to_owned()
    } else {
        format!("/{}{path}", prefix.trim_end_matches('/'))
    };
    let addr_str = if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:80")
    };
    let addr = addr_str
        .to_socket_addrs()
        .map_err(|e| format!("{host}: {e}"))?
        .next()
        .ok_or_else(|| format!("{host}: no address"))?;
    let timeout = Duration::from_secs(5);
    let mut s = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("cannot reach {host}: {e} (is infraplot-server running?)"))?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();
    let req = format!(
        "PUT {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("{host} did not answer HTTP"))?;
    let reply = text
        .split_once("\r\n\r\n")
        .map_or("", |(_, b)| b)
        .to_owned();
    Ok((status, reply))
}

fn put_curl(url: &str, body: &str) -> Result<(u16, String), String> {
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "15",
            "-X",
            "PUT",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            "-w",
            "\n%{http_code}",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("https needs curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (reply, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let status = code
        .trim()
        .parse()
        .map_err(|_| "curl: no status".to_owned())?;
    Ok((status, reply.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn ids_follow_the_server_rules() {
        assert_eq!(diagram_id("Acme HQ network"), "acme-hq-network");
        assert_eq!(diagram_id("--Ñandú!"), "and");
        assert_eq!(diagram_id("!!!"), "diagram");
        assert_eq!(diagram_id(&"x".repeat(80)).len(), 64);
        assert_eq!(normalize_base("localhost:31080/"), "http://localhost:31080");
    }

    #[test]
    fn puts_the_diagram_over_plain_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = vec![0; 65536];
            let mut got = String::new();
            while !got.contains("\"title\"") {
                let n = s.read(&mut buf).unwrap();
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .unwrap();
            got
        });
        let url = publish(&format!("{addr}/infra"), "net", &Diagram::new("T")).unwrap();
        assert_eq!(url, format!("http://{addr}/infra/d/net"));
        let req = server.join().unwrap();
        assert!(
            req.starts_with("PUT /infra/api/diagrams/net HTTP/1.1\r\n"),
            "{req}"
        );
    }
}
