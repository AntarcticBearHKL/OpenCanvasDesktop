//! Integration tests for the AI HTTP layer against an in-process mock server.
//! No test reaches the public internet: every request is served by a
//! `tokio::net::TcpListener` on `127.0.0.1` speaking just enough HTTP/1.1.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use open_canvas_ai::account::{fetch_openrouter_credits_from, fetch_openrouter_key_usage_from};
use open_canvas_ai::client::{build_client, Cancel};
use open_canvas_ai::config::{ModelRequestConfig, ReasoningEffort, IMAGE_MODEL};
use open_canvas_ai::error::AiError;
use open_canvas_ai::generation::fetch_generation_cost;
use open_canvas_ai::image::{request_edit, request_generation, request_image_question};
use open_canvas_ai::plugin_video::{
    create_plugin_video_task, PluginVideoPoll, PluginVideoProvider,
};
use open_canvas_ai::retry::run_with_retry;
use open_canvas_ai::video::{
    create_video_task, create_video_task_with_media, wait_for_video_task, VideoProvider,
};
use open_canvas_ai::video_request::{ReferenceMedia, VideoRequestMedia};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct MockRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    body: String,
}

#[derive(Debug, Clone)]
struct MockResponse {
    status: u16,
    content_type: String,
    body: String,
}

impl MockResponse {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "application/json".to_string(),
            body: body.to_string(),
        }
    }

    fn sse(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream".to_string(),
            body: body.to_string(),
        }
    }

    fn bytes(body: &[u8]) -> Self {
        Self {
            status: 200,
            content_type: "video/mp4".to_string(),
            body: String::from_utf8_lossy(body).to_string(),
        }
    }
}

type Handler = Arc<dyn Fn(&MockRequest) -> MockResponse + Send + Sync>;

struct MockServer {
    addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
    received: Arc<Mutex<Vec<MockRequest>>>,
}

impl MockServer {
    async fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| unreachable!("bind mock server: {error}"));
        let addr = listener
            .local_addr()
            .unwrap_or_else(|error| unreachable!("mock server addr: {error}"));
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&received);
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let handler = Arc::clone(&handler);
                let recorder = Arc::clone(&recorder);
                tokio::spawn(async move {
                    if let Some(request) = read_request(&mut stream).await {
                        if let Ok(mut log) = recorder.lock() {
                            log.push(request.clone());
                        }
                        let response = handler(&request);
                        let _delivery = write_response(&mut stream, &response).await;
                    }
                });
            }
        });
        Self {
            addr,
            handle,
            received,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<MockRequest> {
        self.received
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<MockRequest> {
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
    let authorization = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().to_string())
    });
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
    Some(MockRequest {
        method,
        path,
        authorization,
        body,
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    response: &MockResponse,
) -> std::io::Result<()> {
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

fn config(base_url: &str, model: &str) -> ModelRequestConfig {
    ModelRequestConfig {
        model: model.to_string(),
        base_url: base_url.to_string(),
        api_key: "sk-mock".to_string(),
        system_prompt: "be brief".to_string(),
        reasoning_effort: ReasoningEffort::Auto,
        quality: "auto".to_string(),
        size: "1:1".to_string(),
        background: String::new(),
        count: 2,
        video_seconds: "6".to_string(),
        vquality: "720".to_string(),
        video_generate_audio: "true".to_string(),
        video_watermark: "false".to_string(),
        video_mode: "frames".to_string(),
    }
}

#[tokio::test]
async fn image_generation_posts_the_expected_body_and_parses_images() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/images");
        assert_eq!(request.authorization.as_deref(), Some("Bearer sk-mock"));
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
        assert_eq!(body["model"], serde_json::json!(IMAGE_MODEL));
        assert_eq!(body["n"], serde_json::json!(2));
        assert_eq!(body["output_format"], serde_json::json!("png"));
        assert_eq!(body["prompt"], serde_json::json!("be brief\n\na cat"));
        MockResponse::json(200, r#"{"data":[{"b64_json":"QUJD"}]}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let result = request_generation(
        &client,
        &config(&server.base_url(), "m"),
        "a cat",
        &Cancel::new(),
    )
    .await
    .unwrap_or_else(|error| unreachable!("generation: {error}"));
    assert_eq!(result.images.len(), 1);
    assert_eq!(result.images[0].data_url, "data:image/png;base64,QUJD");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn image_edit_includes_input_references() {
    let handler: Handler = Arc::new(|request| {
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
        assert_eq!(body["n"], serde_json::json!(1));
        assert_eq!(
            body["input_references"][0]["image_url"]["url"],
            serde_json::json!("data:image/png;base64,QUJD")
        );
        MockResponse::json(200, r#"{"images":[{"url":"https://cdn/x.png"}]}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let result = request_edit(
        &client,
        &config(&server.base_url(), "m"),
        "make it blue",
        &["data:image/png;base64,QUJD".to_string()],
        &Cancel::new(),
    )
    .await
    .unwrap_or_else(|error| unreachable!("edit: {error}"));
    assert_eq!(result.images[0].data_url, "https://cdn/x.png");
}

#[tokio::test]
async fn streaming_text_parses_sse_deltas_and_reports_progress() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.path, "/v1/responses");
        assert_eq!(request.authorization.as_deref(), Some("Bearer sk-mock"));
        MockResponse::sse(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\n\
             data: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n\
             data: [DONE]\n\n",
        )
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut deltas: Vec<String> = Vec::new();
    let mut on_delta = |text: &str| deltas.push(text.to_string());
    let response = request_image_question(
        &client,
        &config(&server.base_url(), "text-model"),
        "hi",
        &["data:image/png;base64,QUJD".to_string()],
        &Cancel::new(),
        &mut on_delta,
    )
    .await
    .unwrap_or_else(|error| unreachable!("stream: {error}"));
    assert_eq!(response.content, "Hello");
    assert_eq!(deltas.last().map(String::as_str), Some("Hello"));
    let body: serde_json::Value =
        serde_json::from_str(&server.requests()[0].body).unwrap_or_default();
    assert_eq!(body["stream"], serde_json::json!(true));
    assert_eq!(body["input"][0]["role"], serde_json::json!("system"));
}

#[tokio::test]
async fn http_error_body_is_mapped_to_an_api_error() {
    let handler: Handler =
        Arc::new(|_| MockResponse::json(401, r#"{"error":{"message":"invalid api key"}}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let error = request_generation(
        &client,
        &config(&server.base_url(), "m"),
        "x",
        &Cancel::new(),
    )
    .await
    .err()
    .unwrap_or_else(|| unreachable!("expected error"));
    assert_eq!(error, AiError::Api("invalid api key".to_string()));
}

#[tokio::test]
async fn unreachable_endpoint_is_a_network_error() {
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let config = config("http://127.0.0.1:1", "m");
    let error = request_generation(&client, &config, "x", &Cancel::new())
        .await
        .err()
        .unwrap_or_else(|| unreachable!("expected error"));
    assert!(matches!(error, AiError::Network(_)));
}

#[tokio::test]
async fn retry_succeeds_after_two_server_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let handler: Handler = Arc::new(move |_| {
        let attempt = counter.fetch_add(1, Ordering::SeqCst);
        if attempt < 2 {
            MockResponse::json(500, r#"{"error":{"message":"busy"}}"#)
        } else {
            MockResponse::json(200, r#"{"data":[{"b64_json":"QUJD"}]}"#)
        }
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let config = config(&server.base_url(), "m");
    let cancel = Cancel::new();
    let result = run_with_retry(3, 1, &cancel, |_| {
        request_generation(&client, &config, "x", &cancel)
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn openrouter_video_creates_polls_and_downloads() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&polls);
    let handler: Handler = Arc::new(move |request| match request.path.as_str() {
        "/v1/videos" => {
            MockResponse::json(200, r#"{"id":"task-1","polling_url":"/v1/videos/task-1"}"#)
        }
        "/v1/videos/task-1/content?index=0" => MockResponse::bytes(b"MP4DATA"),
        "/v1/videos/task-1" => {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                MockResponse::json(200, r#"{"status":"pending"}"#)
            } else {
                MockResponse::json(200, r#"{"status":"completed","usage":{"cost":0.25}}"#)
            }
        }
        other => MockResponse::json(
            404,
            &format!(r#"{{"error":{{"message":"no route {other}"}}}}"#),
        ),
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let config = config(&server.base_url(), "minimax/hailuo-3");
    let cancel = Cancel::new();
    let task = create_video_task(&client, &config, "a wave", &[], &cancel)
        .await
        .unwrap_or_else(|error| unreachable!("create: {error}"));
    assert_eq!(task.provider, VideoProvider::OpenRouter);
    assert_eq!(task.id, "task-1");
    let result = wait_for_video_task(&client, &config, &task, &cancel)
        .await
        .unwrap_or_else(|error| unreachable!("wait: {error}"));
    assert_eq!(result.bytes.as_deref(), Some(b"MP4DATA".as_slice()));
    assert_eq!(result.cost_usd, Some(0.25));
}

#[tokio::test]
async fn video_create_surfaces_provider_errors() {
    let handler: Handler =
        Arc::new(|_| MockResponse::json(400, r#"{"error":{"message":"bad prompt"}}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let error = create_video_task(
        &client,
        &config(&server.base_url(), "minimax/hailuo-3"),
        "x",
        &[],
        &Cancel::new(),
    )
    .await
    .err()
    .unwrap_or_else(|| unreachable!("expected error"));
    assert_eq!(error, AiError::Api("bad prompt".to_string()));
}

#[tokio::test]
async fn generation_cost_lookup_parses_typed_fields_including_nulls() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/v1/generation?id=gen-1");
        assert_eq!(request.authorization.as_deref(), Some("Bearer sk-mock"));
        MockResponse::json(
            200,
            r#"{"data":{"total_cost":0.0123,"cache_discount":null,"upstream_inference_cost":0.02,"usage":0.5,"tokens_prompt":100,"tokens_completion":40,"native_tokens_prompt":null,"native_tokens_completion":40}}"#,
        )
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let cost = fetch_generation_cost(
        &client,
        &config(&server.base_url(), "m"),
        "gen-1",
        &Cancel::new(),
    )
    .await
    .unwrap_or_else(|error| unreachable!("cost: {error}"));
    assert_eq!(cost.total_cost, Some(0.0123));
    assert_eq!(cost.cache_discount, None);
    assert_eq!(cost.upstream_inference_cost, Some(0.02));
    assert_eq!(cost.usage, Some(0.5));
    assert_eq!(cost.tokens_prompt, Some(100.0));
    assert_eq!(cost.tokens_completion, Some(40.0));
    assert_eq!(cost.native_tokens_prompt, None);
    assert_eq!(cost.resolved_usd(), Some(0.0123));
}

#[tokio::test]
async fn generation_cost_lookup_falls_back_to_usage() {
    let handler: Handler =
        Arc::new(|_| MockResponse::json(200, r#"{"data":{"total_cost":null,"usage":0.75}}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let cost = fetch_generation_cost(
        &client,
        &config(&server.base_url(), "m"),
        "gen-2",
        &Cancel::new(),
    )
    .await
    .unwrap_or_else(|error| unreachable!("cost: {error}"));
    assert_eq!(cost.total_cost, None);
    assert_eq!(cost.resolved_usd(), Some(0.75));
}

#[tokio::test]
async fn generation_cost_lookup_maps_the_error_envelope() {
    let handler: Handler =
        Arc::new(|_| MockResponse::json(401, r#"{"error":{"message":"invalid api key"}}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let error = fetch_generation_cost(
        &client,
        &config(&server.base_url(), "m"),
        "gen-3",
        &Cancel::new(),
    )
    .await
    .err()
    .unwrap_or_else(|| unreachable!("expected error"));
    assert_eq!(error, AiError::Api("invalid api key".to_string()));
}

#[tokio::test]
async fn account_key_usage_parses_numbers_and_nulls() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/key");
        assert_eq!(request.authorization.as_deref(), Some("Bearer sk-account"));
        MockResponse::json(
            200,
            r#"{"data":{"usage":1.5,"usage_daily":0.1,"usage_weekly":null,"usage_monthly":"2.5","limit":null,"limit_remaining":3,"limit_reset":"2026-01-01T00:00:00Z","is_free_tier":true}}"#,
        )
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let usage =
        fetch_openrouter_key_usage_from(&client, &server.base_url(), "sk-account", &Cancel::new())
            .await
            .unwrap_or_else(|error| unreachable!("account: {error}"));
    assert_eq!(usage.usage, 1.5);
    assert_eq!(usage.usage_daily, 0.1);
    assert_eq!(usage.usage_weekly, 0.0);
    assert_eq!(usage.usage_monthly, 2.5);
    assert_eq!(usage.limit, None);
    assert_eq!(usage.limit_remaining, Some(3.0));
    assert_eq!(usage.limit_reset.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert!(usage.is_free_tier);
}

#[tokio::test]
async fn account_credits_parse_and_map_errors() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.path, "/credits");
        assert_eq!(request.authorization.as_deref(), Some("Bearer sk-account"));
        MockResponse::json(200, r#"{"data":{"total_credits":10,"total_usage":2.5}}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let credits =
        fetch_openrouter_credits_from(&client, &server.base_url(), "sk-account", &Cancel::new())
            .await
            .unwrap_or_else(|error| unreachable!("credits: {error}"));
    assert_eq!(credits.total_credits, 10.0);
    assert_eq!(credits.total_usage, 2.5);
    assert_eq!(credits.remaining(), 7.5);

    let handler: Handler =
        Arc::new(|_| MockResponse::json(403, r#"{"error":{"message":"forbidden"}}"#));
    let server = MockServer::start(handler).await;
    let error =
        fetch_openrouter_credits_from(&client, &server.base_url(), "sk-account", &Cancel::new())
            .await
            .err()
            .unwrap_or_else(|| unreachable!("expected error"));
    assert_eq!(error, AiError::Api("forbidden".to_string()));
}

#[tokio::test]
async fn account_missing_key_fails_before_any_request() {
    let handler: Handler = Arc::new(|_| MockResponse::json(200, "{}"));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let error = fetch_openrouter_key_usage_from(&client, &server.base_url(), "   ", &Cancel::new())
        .await
        .err()
        .unwrap_or_else(|| unreachable!("expected error"));
    assert!(matches!(error, AiError::Invalid(_)));
    assert!(server.requests().is_empty());
}

fn media(name: &str, mime: &str, bytes: &[u8]) -> ReferenceMedia {
    ReferenceMedia::new(name, mime, bytes.to_vec())
}

#[tokio::test]
async fn openai_video_frames_mode_sends_first_and_last_frame() {
    let handler: Handler = Arc::new(|request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/videos");
        assert!(request.body.contains("name=\"first_frame\""));
        assert!(request.body.contains("name=\"last_frame\""));
        assert!(!request.body.contains("name=\"image[]\""));
        assert!(request.body.contains("name=\"mode\""));
        assert!(request.body.contains("frames"));
        MockResponse::json(200, r#"{"id":"job-1"}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut config = config(&server.base_url(), "sora-2");
    config.video_mode = "frames".to_string();
    let media = VideoRequestMedia {
        first_frame: Some(media("f.png", "image/png", b"FIRST")),
        last_frame: Some(media("l.png", "image/png", b"LAST")),
        ..VideoRequestMedia::default()
    };
    let task = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .unwrap_or_else(|error| unreachable!("create: {error}"));
    assert_eq!(task.id, "job-1");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn openai_video_reference_mode_sends_image_video_and_audio_arrays() {
    let handler: Handler = Arc::new(|request| {
        assert!(request.body.contains("name=\"image[]\""));
        assert!(request.body.contains("name=\"video[]\""));
        assert!(request.body.contains("name=\"audio[]\""));
        assert!(!request.body.contains("name=\"first_frame\""));
        assert!(request.body.contains("reference"));
        MockResponse::json(200, r#"{"id":"job-2"}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut config = config(&server.base_url(), "sora-2");
    config.video_mode = "reference".to_string();
    let media = VideoRequestMedia {
        images: vec![media("i.png", "image/png", b"IMG")],
        videos: vec![media("v.mp4", "video/mp4", b"VID")],
        audios: vec![media("a.mp3", "audio/mpeg", b"AUD")],
        ..VideoRequestMedia::default()
    };
    let task = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .unwrap_or_else(|error| unreachable!("create: {error}"));
    assert_eq!(task.id, "job-2");
}

#[tokio::test]
async fn openrouter_video_body_carries_reference_parts() {
    let handler: Handler = Arc::new(|request| {
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
        assert_eq!(
            body["input_references"][0]["type"],
            serde_json::json!("image_url")
        );
        assert_eq!(
            body["input_references"][1]["type"],
            serde_json::json!("video_url")
        );
        assert!(body.get("frame_images").is_none());
        MockResponse::json(200, r#"{"id":"t-1"}"#)
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut config = config(&server.base_url(), "minimax/hailuo-3");
    config.video_mode = "reference".to_string();
    let media = VideoRequestMedia {
        images: vec![media("i.png", "image/png", b"IMG")],
        videos: vec![media("v.mp4", "video/mp4", b"VID").with_duration_ms(3_000)],
        ..VideoRequestMedia::default()
    };
    let task = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .unwrap_or_else(|error| unreachable!("create: {error}"));
    assert_eq!(task.id, "t-1");
}

#[tokio::test]
async fn openrouter_audio_only_reference_is_rejected_without_a_request() {
    let handler: Handler = Arc::new(|_| MockResponse::json(200, r#"{"id":"never"}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut config = config(&server.base_url(), "minimax/hailuo-3");
    config.video_mode = "reference".to_string();
    let media = VideoRequestMedia {
        audios: vec![media("a.mp3", "audio/mpeg", b"AUD")],
        ..VideoRequestMedia::default()
    };
    let error = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .err()
        .unwrap_or_else(|| unreachable!("expected error"));
    assert!(matches!(error, AiError::Invalid(message) if message.contains("alone")));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn openrouter_reference_limits_reject_before_any_request() {
    let handler: Handler = Arc::new(|_| MockResponse::json(200, r#"{"id":"never"}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let config = config(&server.base_url(), "minimax/hailuo-3");
    let media = VideoRequestMedia {
        images: (0..10)
            .map(|index| media(&format!("i{index}.png"), "image/png", b"IMG"))
            .collect(),
        ..VideoRequestMedia::default()
    };
    let error = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .err()
        .unwrap_or_else(|| unreachable!("expected error"));
    assert!(matches!(error, AiError::Invalid(_)));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn openrouter_reference_seconds_window_rejects_before_any_request() {
    let handler: Handler = Arc::new(|_| MockResponse::json(200, r#"{"id":"never"}"#));
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let mut config = config(&server.base_url(), "minimax/hailuo-3");
    config.video_mode = "reference".to_string();
    let media = VideoRequestMedia {
        images: vec![media("i.png", "image/png", b"IMG")],
        videos: vec![media("v.mp4", "video/mp4", b"VID").with_duration_ms(1_000)],
        ..VideoRequestMedia::default()
    };
    let error = create_video_task_with_media(&client, &config, "a wave", &media, &Cancel::new())
        .await
        .err()
        .unwrap_or_else(|| unreachable!("expected error"));
    assert!(matches!(error, AiError::Invalid(message) if message.contains("2-15 seconds")));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn plugin_video_declarative_provider_creates_polls_and_downloads() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&polls);
    let handler: Handler = Arc::new(move |request| match request.path.as_str() {
        "/v1/videos" => {
            let body: serde_json::Value = serde_json::from_str(&request.body).unwrap_or_default();
            assert_eq!(body["model"], serde_json::json!("plugin-video"));
            assert_eq!(body["prompt"], serde_json::json!("a wave"));
            assert_eq!(body["mode"], serde_json::json!("frames"));
            assert_eq!(body["audio"], serde_json::json!(true));
            MockResponse::json(200, r#"{"task_id":"p-1"}"#)
        }
        "/v1/videos/p-1" => {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                MockResponse::json(200, r#"{"state":"running"}"#)
            } else {
                MockResponse::json(200, r#"{"state":"finished","asset":"/v1/asset/p-1"}"#)
            }
        }
        "/v1/asset/p-1" => MockResponse::bytes(b"PLUGINMP4"),
        other => MockResponse::json(
            404,
            &format!(r#"{{"error":{{"message":"no route {other}"}}}}"#),
        ),
    });
    let server = MockServer::start(handler).await;
    let client = build_client().unwrap_or_else(|error| unreachable!("client: {error}"));
    let config = config(&server.base_url(), "plugin-video");
    let provider = PluginVideoProvider {
        endpoint: "/videos".to_string(),
        body: serde_json::json!({
            "model": "{{model}}",
            "prompt": "{{prompt}}",
            "mode": "{{mode}}",
            "audio": "{{generateAudio}}"
        }),
        poll: PluginVideoPoll {
            endpoint: "/videos/{{id}}".to_string(),
            id_path: "task_id".to_string(),
            status_path: "state".to_string(),
            completed: vec!["finished".to_string()],
            failed: vec!["error".to_string()],
            result_url_path: "asset".to_string(),
            interval_ms: 1,
            timeout_ms: 2_000,
        },
    };
    let result = create_plugin_video_task(
        &client,
        &config,
        &provider,
        "a wave",
        &VideoRequestMedia::default(),
        &Cancel::new(),
    )
    .await
    .unwrap_or_else(|error| unreachable!("plugin video: {error}"));
    assert_eq!(result.bytes.as_deref(), Some(b"PLUGINMP4".as_slice()));
}
