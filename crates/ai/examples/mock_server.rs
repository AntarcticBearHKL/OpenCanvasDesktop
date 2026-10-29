//! A tiny OpenAI-compatible mock server for the P5a screenshots: image
//! generation, streaming text, and video create/poll/content, all served from
//! `127.0.0.1` with no external calls. A prompt containing `fail` returns HTTP
//! 500 so the error path can be captured too.
//!
//! Run: `cargo run -p open-canvas-ai --example mock_server -- 8799`
//! Stop: kill the process (the capture script owns its lifetime).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 1x1 transparent PNG.
const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

struct Request {
    method: String,
    path: String,
    body: String,
}

#[tokio::main]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8799);
    let polls = Arc::new(AtomicUsize::new(0));
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("mock_server: bind failed: {error}");
            return;
        }
    };
    println!("mock_server listening on 127.0.0.1:{port}");
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            continue;
        };
        let polls = Arc::clone(&polls);
        tokio::spawn(async move {
            if let Some(request) = read_request(&mut stream).await {
                eprintln!("mock_server: {} {}", request.method, request.path);
                // A deliberate delay keeps the "loading" state on screen long
                // enough to capture; the failing prompt answers immediately so
                // the error path does not stack retry delays.
                let failing = request
                    .body
                    .to_ascii_lowercase()
                    .contains("\"prompt\":\"fail");
                if request.path == "/v1/images" && !failing {
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                }
                let response = route(&request, &polls);
                let delivered = write_response(&mut stream, &response).await;
                if let Err(error) = delivered {
                    eprintln!("mock_server: write failed: {error}");
                }
            }
        });
    }
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: String,
}

fn json(status: u16, body: &str) -> Response {
    Response {
        status,
        content_type: "application/json",
        body: body.to_string(),
    }
}

fn route(request: &Request, polls: &AtomicUsize) -> Response {
    if request.path == "/v1/images" {
        if request
            .body
            .to_ascii_lowercase()
            .contains("\"prompt\":\"fail")
        {
            return json(500, r#"{"error":{"message":"mock image failure"}}"#);
        }
        return json(
            200,
            &format!(r#"{{"data":[{{"b64_json":"{PNG_B64}","media_type":"image/png"}}]}}"#),
        );
    }
    if request.path == "/v1/responses" {
        let body = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Mock \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"streamed answer\"}\n\n\
data: [DONE]\n\n";
        return Response {
            status: 200,
            content_type: "text/event-stream",
            body: body.to_string(),
        };
    }
    if request.path == "/v1/videos" {
        return json(200, r#"{"id":"mock-1","polling_url":"/v1/videos/mock-1"}"#);
    }
    if request.path == "/v1/videos/mock-1" {
        if polls.fetch_add(1, Ordering::SeqCst) == 0 {
            return json(200, r#"{"status":"pending"}"#);
        }
        return json(200, r#"{"status":"completed","usage":{"cost":0.1}}"#);
    }
    if request.path.starts_with("/v1/videos/mock-1/content") {
        return Response {
            status: 200,
            content_type: "video/mp4",
            body: "MOCKMP4".to_string(),
        };
    }
    let _method = &request.method;
    json(404, r#"{"error":{"message":"no route"}}"#)
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(
        &buffer[header_end..(header_end + content_length).min(buffer.len())],
    )
    .to_string();
    Some(Request { method, path, body })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

async fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(response.body.as_bytes()).await?;
    stream.flush().await
}
