use std::io::{Error, ErrorKind};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub headers: Vec<(&'a str, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

pub async fn send(origin: &str, request: Request<'_>) -> Result<Reply, Error> {
    match tokio::time::timeout(request.timeout, exchange(origin, &request)).await {
        Ok(reply) => reply,
        Err(_) => Err(Error::from(ErrorKind::TimedOut)),
    }
}

async fn exchange(origin: &str, request: &Request<'_>) -> Result<Reply, Error> {
    let host = origin.strip_prefix("http://").unwrap_or(origin);
    let mut stream = TcpStream::connect(host).await?;
    let mut head = format!(
        "{} {} HTTP/1.1\r\nhost: {host}\r\nconnection: close\r\ncontent-length: {}\r\n",
        request.method,
        request.path,
        request.body.len()
    );
    for (name, value) in &request.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&request.body).await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    parse(&raw)
}

fn parse(raw: &[u8]) -> Result<Reply, Error> {
    let broken = || Error::from(ErrorKind::InvalidData);
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(broken)?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| broken())?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(broken)?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let rest = &raw[split + 4..];
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
    let body = if chunked {
        unchunk(rest).ok_or_else(broken)?
    } else {
        rest.to_vec()
    };
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn unchunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = rest.windows(2).position(|window| window == b"\r\n")?;
        let size = std::str::from_utf8(&rest[..line_end]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Some(body);
        }
        body.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}
