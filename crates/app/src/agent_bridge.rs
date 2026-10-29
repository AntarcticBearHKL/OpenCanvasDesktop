//! Local HTTP bridge for external agents.
//!
//! The desktop counterpart of the web app's browser bridge: the Python MCP
//! server in `mcp/` speaks HTTP to this loopback-only endpoint, and the MCP
//! server turns the tool surface into ordinary project JSON reads/writes.
//!
//! Why a bridge instead of a second process over the data directory: the app
//! owns the open document in memory and autosaves it, so an external writer
//! that only touched `projects\<id>.json` would either be overwritten or be
//! invisible until a restart. Every request here is executed on the UI thread
//! through the same `App` methods the UI uses, then synced into the window.
//!
//! Layout (default `127.0.0.1:3211`):
//!
//! ```text
//! GET    /health
//! GET    /state
//! GET    /library
//! GET    /project/<id>
//! PUT    /project/<id>                 full CanvasProject JSON
//! POST   /library/projects             { title?, groupId? }
//! PATCH  /library/projects/<id>        { title?, groupId? | null }
//! DELETE /library/projects/<id>
//! POST   /library/groups               { name? }
//! PATCH  /library/groups/<id>          { name }
//! DELETE /library/groups/<id>
//! POST   /project/<id>/open
//! POST   /selection                    { ids: [string] }
//! POST   /project/<id>/generate        { nodeId }
//! POST   /project/<id>/cancel          { nodeId }
//! GET    /generation
//! GET    /screenshot                    PNG bytes
//! ```
//!
//! `OC_AGENT_BRIDGE=0` disables it; `OC_AGENT_BRIDGE_PORT`,
//! `OC_AGENT_BRIDGE_HOST` and `OC_AGENT_BRIDGE_TOKEN` configure it. The token,
//! when set, is required as `Authorization: Bearer <token>`.

use std::cell::RefCell;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use open_canvas_core::document::{CanvasGroup, CanvasProject, ProjectIndexEntry};
use open_canvas_core::node::CanvasNodeType;
use serde_json::{json, Value};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::app::{App, View};
use crate::doc::Doc;
use crate::library::EditorMeta;
use crate::store;
use crate::AppWindow;

/// Wire protocol version shared with the Python MCP server.
pub const PROTOCOL_VERSION: u32 = 1;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3211;
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PUMP_INTERVAL: Duration = Duration::from_millis(8);

/// Starts the bridge for this window. Binding errors are reported to stderr and
/// are not fatal: the app must still work when the port is taken.
pub fn start(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    let Some(settings) = Settings::from_env() else {
        return;
    };
    let (sender, receiver) = mpsc::channel::<Job>();
    if let Err(error) = spawn_server(settings, sender) {
        eprintln!("agent bridge: cannot start: {error}");
        return;
    }

    let weak_app = Rc::downgrade(app);
    let weak_ui = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, PUMP_INTERVAL, move || {
        drain_jobs(&receiver, &weak_app, &weak_ui);
    });
    if let Ok(mut app) = app.try_borrow_mut() {
        app.keep_timer(timer);
    }
}

fn drain_jobs(
    receiver: &Receiver<Job>,
    weak_app: &std::rc::Weak<RefCell<App>>,
    weak_ui: &slint::Weak<AppWindow>,
) {
    loop {
        let Ok(job) = receiver.try_recv() else {
            break;
        };
        let Some(app) = weak_app.upgrade() else {
            break;
        };
        let Some(ui) = weak_ui.upgrade() else {
            break;
        };
        let reply = match app.try_borrow_mut() {
            Ok(mut app) => handle(&mut app, &ui, job.command),
            Err(_) => Reply::error(503, "the app is busy; retry the request"),
        };
        drop(job.reply.send(reply));
    }
}

#[derive(Debug, Clone)]
struct Settings {
    host: String,
    port: u16,
    token: String,
}

impl Settings {
    fn from_env() -> Option<Self> {
        if env_flag_off("OC_AGENT_BRIDGE") {
            return None;
        }
        let host = std::env::var("OC_AGENT_BRIDGE_HOST")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_HOST.to_string());
        let port = std::env::var("OC_AGENT_BRIDGE_PORT")
            .ok()
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let token = std::env::var("OC_AGENT_BRIDGE_TOKEN")
            .unwrap_or_default()
            .trim()
            .to_string();
        Some(Self { host, port, token })
    }
}

fn env_flag_off(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no" | "disabled"
        )
    })
}

fn spawn_server(settings: Settings, sender: Sender<Job>) -> io::Result<()> {
    let listener = TcpListener::bind((settings.host.as_str(), settings.port))?;
    let token = Arc::new(settings.token);
    eprintln!(
        "agent bridge listening on http://{}:{}",
        settings.host, settings.port
    );
    std::thread::Builder::new()
        .name("open-canvas-agent-bridge".to_string())
        .spawn(move || {
            for incoming in listener.incoming() {
                match incoming {
                    Ok(stream) => {
                        let sender = sender.clone();
                        let token = Arc::clone(&token);
                        let spawned = std::thread::Builder::new()
                            .name("open-canvas-agent-request".to_string())
                            .spawn(move || {
                                if let Err(error) = serve_connection(stream, sender, token.as_str())
                                {
                                    eprintln!("agent bridge request: {error}");
                                }
                            });
                        if let Err(error) = spawned {
                            eprintln!("agent bridge: cannot spawn request thread: {error}");
                        }
                    }
                    Err(error) => eprintln!("agent bridge accept: {error}"),
                }
            }
        })
        .map(|_| ())
}

#[derive(Debug)]
struct Job {
    command: Command,
    reply: Sender<Reply>,
}

#[derive(Debug)]
enum Command {
    Health,
    State,
    Library,
    GetProject(String),
    PutProject(String, Value),
    CreateProject(Value),
    PatchProject(String, Value),
    DeleteProject(String),
    CreateGroup(Value),
    RenameGroup(String, Value),
    DeleteGroup(String),
    OpenProject(String),
    SetSelection(Value),
    Generate(String, Value),
    Cancel(String, Value),
    GenerationStatus,
    Screenshot,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Debug)]
struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, value: &Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    fn ok(value: &Value) -> Self {
        Self::json(200, value)
    }

    fn error(status: u16, message: impl Into<String>) -> Self {
        Self::json(status, &json!({ "ok": false, "error": message.into() }))
    }

    fn png(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: "image/png",
            body,
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the request thread owns the connection and its reply sender"
)]
fn serve_connection(mut stream: TcpStream, sender: Sender<Job>, token: &str) -> io::Result<()> {
    drop(stream.set_read_timeout(Some(Duration::from_secs(15))));
    drop(stream.set_write_timeout(Some(Duration::from_secs(30))));
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };
    if !authorized(&request, token) {
        return write_reply(stream, Reply::error(401, "missing or invalid bearer token"));
    }
    let command = match route(&request) {
        Ok(command) => command,
        Err((status, message)) => return write_reply(stream, Reply::error(status, message)),
    };
    let (reply_sender, reply_receiver) = mpsc::channel();
    if sender
        .send(Job {
            command,
            reply: reply_sender,
        })
        .is_err()
    {
        return write_reply(
            stream,
            Reply::error(503, "the app event loop is not running"),
        );
    }
    let reply = reply_receiver
        .recv_timeout(REQUEST_TIMEOUT)
        .unwrap_or_else(|_| Reply::error(504, "the app did not answer in time"));
    write_reply(stream, reply)
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let target = parts.next().unwrap_or("/").to_string();
    if method.is_empty() || !target.starts_with('/') {
        return Ok(None);
    }
    let path = target.split('?').next().unwrap_or("/").to_string();
    let mut headers = Vec::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if name == "content-length" {
            content_length = value.parse::<usize>().unwrap_or(0);
        }
        headers.push((name, value));
    }
    if content_length > MAX_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request body is too large",
        ));
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(HttpRequest {
        method,
        path,
        headers,
        body,
    }))
}

fn authorized(request: &HttpRequest, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }
    request
        .headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .is_some_and(|(_, value)| {
            value
                .strip_prefix("Bearer ")
                .is_some_and(|candidate| candidate == token)
        })
}

fn route(request: &HttpRequest) -> Result<Command, (u16, String)> {
    let parts: Vec<&str> = request
        .path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    match (request.method.as_str(), parts.as_slice()) {
        ("GET", ["health"]) => Ok(Command::Health),
        ("GET", ["state"]) => Ok(Command::State),
        ("GET", ["library"]) => Ok(Command::Library),
        ("GET", ["generation"]) => Ok(Command::GenerationStatus),
        ("GET", ["screenshot"]) => Ok(Command::Screenshot),
        ("GET", ["project", id]) => Ok(Command::GetProject((*id).to_string())),
        ("PUT", ["project", id]) => Ok(Command::PutProject((*id).to_string(), body_json(request)?)),
        ("POST", ["library", "projects"]) => Ok(Command::CreateProject(body_json(request)?)),
        ("PATCH", ["library", "projects", id]) => Ok(Command::PatchProject(
            (*id).to_string(),
            body_json(request)?,
        )),
        ("DELETE", ["library", "projects", id]) => Ok(Command::DeleteProject((*id).to_string())),
        ("POST", ["library", "groups"]) => Ok(Command::CreateGroup(body_json(request)?)),
        ("PATCH", ["library", "groups", id]) => {
            Ok(Command::RenameGroup((*id).to_string(), body_json(request)?))
        }
        ("DELETE", ["library", "groups", id]) => Ok(Command::DeleteGroup((*id).to_string())),
        ("POST", ["project", id, "open"]) => Ok(Command::OpenProject((*id).to_string())),
        ("POST", ["selection"]) => Ok(Command::SetSelection(body_json(request)?)),
        ("POST", ["project", id, "generate"]) => {
            Ok(Command::Generate((*id).to_string(), body_json(request)?))
        }
        ("POST", ["project", id, "cancel"]) => {
            Ok(Command::Cancel((*id).to_string(), body_json(request)?))
        }
        (_, []) => Err((404, "unknown endpoint".to_string())),
        _ => Err((
            404,
            format!("no route for {} {}", request.method, request.path),
        )),
    }
}

fn body_json(request: &HttpRequest) -> Result<Value, (u16, String)> {
    if request.body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&request.body)
        .map_err(|error| (400, format!("invalid JSON body: {error}")))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the reply owns its body and is written once"
)]
fn write_reply(mut stream: TcpStream, reply: Reply) -> io::Result<()> {
    let reason = match reply.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        reply.status,
        reason,
        reply.content_type,
        reply.body.len()
    )?;
    stream.write_all(&reply.body)?;
    stream.flush()
}

fn handle(app: &mut App, ui: &AppWindow, command: Command) -> Reply {
    match command {
        Command::Health => Reply::ok(&json!({
            "ok": true,
            "app": "open-canvas-desktop",
            "protocolVersion": PROTOCOL_VERSION,
            "version": env!("CARGO_PKG_VERSION"),
            "dataDir": app.store.root().display().to_string(),
        })),
        Command::State => Reply::ok(&state_json(app)),
        Command::Library => Reply::ok(&library_json(app)),
        Command::GetProject(id) => match project_json(app, &id) {
            Ok(value) => Reply::ok(&value),
            Err(reply) => reply,
        },
        Command::PutProject(id, body) => put_project(app, ui, &id, body),
        Command::CreateProject(body) => create_project(app, ui, &body),
        Command::PatchProject(id, body) => patch_project(app, ui, &id, &body),
        Command::DeleteProject(id) => delete_project(app, ui, &id),
        Command::CreateGroup(body) => create_group(app, ui, &body),
        Command::RenameGroup(id, body) => rename_group(app, ui, &id, &body),
        Command::DeleteGroup(id) => delete_group(app, ui, &id),
        Command::OpenProject(id) => open_project(app, ui, &id),
        Command::SetSelection(body) => set_selection(app, ui, &body),
        Command::Generate(id, body) => generate(app, ui, &id, &body),
        Command::Cancel(id, body) => cancel(app, ui, &id, &body),
        Command::GenerationStatus => Reply::ok(&json!({
            "running": app.panel.running,
            "runtimeReady": app.ai.is_some(),
            "openProjectId": app.editor.as_ref().map(|meta| meta.id.clone()),
            "view": view_name(app.view),
        })),
        Command::Screenshot => screenshot(ui),
    }
}

fn view_name(view: View) -> &'static str {
    match view {
        View::Library => "library",
        View::Editor => "editor",
        View::Image => "image",
    }
}

fn state_json(app: &App) -> Value {
    let canvas_rows = app
        .library
        .index
        .projects
        .iter()
        .filter(|row| row.kind == "canvas")
        .count();
    let image_rows = app.library.index.projects.len().saturating_sub(canvas_rows);
    json!({
        "app": "open-canvas-desktop",
        "protocolVersion": PROTOCOL_VERSION,
        "view": view_name(app.view),
        "openProjectId": app.editor.as_ref().map(|meta| meta.id.clone()),
        "openProjectTitle": app.editor.as_ref().map(|meta| meta.title.clone()),
        "nodeCount": app.doc.nodes().len(),
        "connectionCount": app.doc.connections().len(),
        "selection": app.doc.selection().iter().cloned().collect::<Vec<String>>(),
        "runningNodeId": app.panel.running,
        "library": {
            "groups": app.library.index.groups.len(),
            "canvasProjects": canvas_rows,
            "imageProjects": image_rows,
        },
        "capabilities": {
            "canvas": true,
            "imageStudio": true,
            "generation": app.ai.is_some(),
            "screenshot": true,
        }
    })
}

fn library_json(app: &App) -> Value {
    let mut value = serde_json::to_value(&app.library.index).unwrap_or_else(|_| json!({}));
    if let Value::Object(ref mut map) = value {
        map.insert(
            "openProjectId".to_string(),
            app.editor
                .as_ref()
                .map_or(Value::Null, |meta| Value::String(meta.id.clone())),
        );
        map.insert(
            "view".to_string(),
            Value::String(view_name(app.view).to_string()),
        );
    }
    value
}

fn project_json(app: &App, id: &str) -> Result<Value, Reply> {
    let is_open = app.editor.as_ref().is_some_and(|meta| meta.id == id);
    let project = if is_open {
        let Some(meta) = app.editor.as_ref() else {
            return Err(Reply::error(500, "the editor metadata disappeared"));
        };
        app.doc.to_project(meta)
    } else {
        let row = app.library.index.project(id);
        match row {
            Some(row) if row.kind != "canvas" => {
                return Err(Reply::error(409, "only canvas projects are supported"));
            }
            Some(_) => {}
            None => return Err(Reply::error(404, format!("unknown project {id}"))),
        }
        app.store
            .load_project(id)
            .map_err(|error| Reply::error(404, error.to_string()))?
    };
    serde_json::to_value(project)
        .map_err(|error| Reply::error(500, format!("cannot serialize project: {error}")))
}

fn put_project(app: &mut App, ui: &AppWindow, id: &str, body: Value) -> Reply {
    let mut project: CanvasProject = match serde_json::from_value(body) {
        Ok(project) => project,
        Err(error) => return Reply::error(400, format!("invalid project document: {error}")),
    };
    if project.id != id {
        project.id = id.to_string();
    }
    if let Some(row) = app.library.index.project(id) {
        if row.kind != "canvas" {
            return Reply::error(409, "only canvas projects are supported");
        }
    } else {
        return Reply::error(404, format!("unknown project {id}"));
    }
    // Agent writes go through the same destructive migration as project open:
    // a bridge client can never persist text/video or old prompt graphs.
    let mut doc = Doc::from_project(&project);
    if !doc.migrate_legacy_workflows().is_empty() {
        let meta = crate::library::EditorMeta {
            id: project.id.clone(),
            title: project.title.clone(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            group_id: project.group_id.clone(),
        };
        project = doc.to_project(&meta);
    }
    project.updated_at = store::now_iso();
    if let Err(error) = app.store.save_project(&project) {
        return Reply::error(500, error.to_string());
    }
    app.library.index.upsert_project(&project);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.library.side_counts.remove(id);
    let is_open = app.editor.as_ref().is_some_and(|meta| meta.id == id);
    if is_open {
        let previous_selection: Vec<String> = app.doc.selection().iter().cloned().collect();
        app.doc = Doc::from_project(&project);
        for selected in previous_selection {
            if app.doc.node(&selected).is_some() {
                app.doc.select_node(&selected, true);
            }
        }
        app.editor = Some(EditorMeta {
            id: project.id.clone(),
            title: project.title.clone(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            group_id: project.group_id.clone(),
        });
        app.save_dirty = false;
        app.save_due = None;
        app.sync_all(ui);
    } else {
        app.sync_library(ui);
    }
    serde_json::to_value(&project).map_or_else(
        |error| Reply::error(500, format!("cannot serialize project: {error}")),
        |value| Reply::ok(&value),
    )
}

fn create_project(app: &mut App, ui: &AppWindow, body: &Value) -> Reply {
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map_or_else(
            || format!("Canvas {}", app.library.index.projects.len() + 1),
            str::to_string,
        );
    let group_id = body
        .get("groupId")
        .or_else(|| body.get("libraryId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(group_id) = group_id else {
        return Reply::error(400, "groupId (or libraryId) is required");
    };
    if app.library.index.group(&group_id).is_none() {
        return Reply::error(404, format!("unknown group {group_id}"));
    }
    let id = next_id(app, "project");
    let stamp = store::now_iso();
    let mut project = CanvasProject::new(id.clone(), title, stamp.clone(), stamp);
    project.group_id = Some(group_id);
    if let Err(error) = app.store.save_project(&project) {
        return Reply::error(500, error.to_string());
    }
    app.library.index.upsert_project(&project);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.library.side_counts.remove(&id);
    app.sync_library(ui);
    serde_json::to_value(&project).map_or_else(
        |error| Reply::error(500, format!("cannot serialize project: {error}")),
        |value| Reply::ok(&value),
    )
}

fn patch_project(app: &mut App, ui: &AppWindow, id: &str, body: &Value) -> Reply {
    let is_open = app.editor.as_ref().is_some_and(|meta| meta.id == id);
    let mut project = if is_open {
        let Some(meta) = app.editor.as_ref() else {
            return Reply::error(500, "the editor metadata disappeared");
        };
        app.doc.to_project(meta)
    } else {
        match app.store.load_project(id) {
            Ok(project) => project,
            Err(error) => return Reply::error(404, error.to_string()),
        }
    };
    if let Some(title) = body.get("title").and_then(Value::as_str) {
        let title = title.trim();
        if title.is_empty() {
            return Reply::error(400, "title cannot be empty");
        }
        project.title = title.to_string();
    }
    if body.get("groupId").is_some() {
        match body.get("groupId") {
            Some(Value::Null) => project.group_id = None,
            Some(Value::String(group_id)) => {
                if app.library.index.group(group_id).is_none() {
                    return Reply::error(404, format!("unknown group {group_id}"));
                }
                project.group_id = Some(group_id.clone());
            }
            Some(_) => return Reply::error(400, "groupId must be a string or null"),
            None => {}
        }
    }
    project.updated_at = store::now_iso();
    if let Err(error) = app.store.save_project(&project) {
        return Reply::error(500, error.to_string());
    }
    app.library.index.upsert_project(&project);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.library.side_counts.remove(id);
    if is_open {
        if let Some(meta) = app.editor.as_mut() {
            meta.title.clone_from(&project.title);
            meta.updated_at.clone_from(&project.updated_at);
            meta.group_id.clone_from(&project.group_id);
        }
        app.sync_all(ui);
    } else {
        app.sync_library(ui);
    }
    serde_json::to_value(&project).map_or_else(
        |error| Reply::error(500, format!("cannot serialize project: {error}")),
        |value| Reply::ok(&value),
    )
}

fn delete_project(app: &mut App, ui: &AppWindow, id: &str) -> Reply {
    let Some(row) = app.library.index.project(id).cloned() else {
        return Reply::error(404, format!("unknown project {id}"));
    };
    if app.editor.as_ref().is_some_and(|meta| meta.id == id) {
        app.go_home(ui);
    }
    if let Err(error) = app.store.delete_by_kind(id, &row.kind) {
        return Reply::error(500, error.to_string());
    }
    app.library.index.remove_project(id);
    app.library.side_counts.remove(id);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.sync_library(ui);
    Reply::ok(&json!({ "deleted": [id] }))
}

fn create_group(app: &mut App, ui: &AppWindow, body: &Value) -> Reply {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || format!("Project {}", app.library.index.groups.len() + 1),
            str::to_string,
        );
    let id = next_id(app, "group");
    let group = CanvasGroup {
        id: id.clone(),
        name,
        created_at: store::now_iso(),
    };
    app.library.index.groups.push(group.clone());
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.sync_library(ui);
    serde_json::to_value(&group).map_or_else(
        |error| Reply::error(500, format!("cannot serialize group: {error}")),
        |value| Reply::ok(&value),
    )
}

fn rename_group(app: &mut App, ui: &AppWindow, id: &str, body: &Value) -> Reply {
    let Some(name) = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
    else {
        return Reply::error(400, "name cannot be empty");
    };
    let Some(group) = app
        .library
        .index
        .groups
        .iter_mut()
        .find(|group| group.id == id)
    else {
        return Reply::error(404, format!("unknown group {id}"));
    };
    group.name = name;
    let value = serde_json::to_value(&*group).unwrap_or(Value::Null);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.sync_library(ui);
    Reply::ok(&value)
}

fn delete_group(app: &mut App, ui: &AppWindow, id: &str) -> Reply {
    if app.library.index.group(id).is_none() {
        return Reply::error(404, format!("unknown group {id}"));
    }
    let doomed: Vec<ProjectIndexEntry> = app
        .library
        .index
        .projects
        .iter()
        .filter(|row| row.group_id.as_deref() == Some(id))
        .cloned()
        .collect();
    if app
        .editor
        .as_ref()
        .is_some_and(|meta| doomed.iter().any(|row| row.id == meta.id))
    {
        app.go_home(ui);
    }
    if app
        .image
        .meta
        .as_ref()
        .is_some_and(|meta| doomed.iter().any(|row| row.id == meta.id))
    {
        app.go_home(ui);
    }
    let mut deleted = Vec::new();
    for row in &doomed {
        if let Err(error) = app.store.delete_by_kind(&row.id, &row.kind) {
            return Reply::error(500, error.to_string());
        }
        app.library.side_counts.remove(&row.id);
        deleted.push(row.id.clone());
    }
    app.library
        .index
        .projects
        .retain(|row| row.group_id.as_deref() != Some(id));
    app.library.index.groups.retain(|group| group.id != id);
    if let Err(error) = app.store.save_index(&app.library.index) {
        return Reply::error(500, error.to_string());
    }
    app.sync_library(ui);
    Reply::ok(&json!({ "deletedProjects": deleted }))
}

fn open_project(app: &mut App, ui: &AppWindow, id: &str) -> Reply {
    match app.store.load_project(id) {
        Ok(project) => {
            app.open_project_ui(ui, project);
            Reply::ok(&state_json(app))
        }
        Err(error) => Reply::error(404, error.to_string()),
    }
}

fn set_selection(app: &mut App, ui: &AppWindow, body: &Value) -> Reply {
    let Some(ids) = body.get("ids").and_then(Value::as_array) else {
        return Reply::error(400, "ids must be an array");
    };
    let selected: Vec<String> = ids
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| app.doc.node(id).is_some())
        .map(str::to_string)
        .collect();
    app.doc.clear_selection();
    for id in &selected {
        app.doc.select_node(id, true);
    }
    app.sync_selection(ui);
    app.sync_flags(ui);
    Reply::ok(&json!({ "selected": selected }))
}

fn generate(app: &mut App, ui: &AppWindow, id: &str, body: &Value) -> Reply {
    let Some(node_id) = body.get("nodeId").and_then(Value::as_str) else {
        return Reply::error(400, "nodeId is required");
    };
    if app.editor.as_ref().is_none_or(|meta| meta.id != id) {
        return Reply::error(
            409,
            format!("project {id} must be open to run a generation"),
        );
    }
    if app.doc.node(node_id).is_none() {
        return Reply::error(404, format!("unknown node {node_id}"));
    }
    if app.panel.running.as_deref() == Some(node_id) {
        return Reply::ok(
            &json!({ "started": false, "reason": "already running", "nodeId": node_id }),
        );
    }
    // Agent-facing attempt updates: the agent supplies the prompt/config diff
    // and its raw trace before asking for generation. A frozen attempt only
    // accepts a new run with the same config; changing it must go through a
    // child attempt.
    if let Some(node) = app
        .doc
        .nodes_mut()
        .iter_mut()
        .find(|node| node.id == node_id)
    {
        if node.node_type == CanvasNodeType::ImageAttempt {
            let frozen = node
                .metadata
                .as_ref()
                .and_then(|meta| meta.attempt_frozen)
                .unwrap_or(false);
            let meta = node.metadata.get_or_insert_with(Default::default);
            if let Some(trace) = body.get("agentTrace").and_then(Value::as_str) {
                meta.agent_raw_trace = Some(trace.to_string());
            }
            if let Some(run_id) = body.get("agentRunId").and_then(Value::as_str) {
                meta.agent_run_id = Some(run_id.to_string());
            }
            if let Some(steps) = body.get("agentSteps").and_then(Value::as_array) {
                meta.agent_steps = Some(
                    steps
                        .iter()
                        .filter_map(|step| {
                            let title = step.get("title")?.as_str()?.to_string();
                            Some(open_canvas_core::node::CanvasAgentStep {
                                kind: step
                                    .get("kind")
                                    .and_then(Value::as_str)
                                    .unwrap_or("step")
                                    .to_string(),
                                title,
                                detail: step
                                    .get("detail")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                at: step.get("at").and_then(Value::as_str).map(str::to_string),
                            })
                        })
                        .collect(),
                );
            }
            if !frozen {
                if let Some(prompt) = body.get("prompt").and_then(Value::as_str) {
                    meta.prompt = Some(prompt.to_string());
                }
                if let Some(model) = body.get("model").and_then(Value::as_str) {
                    meta.model = Some(model.to_string());
                }
                if let Some(size) = body.get("size").and_then(Value::as_str) {
                    meta.size = Some(size.to_string());
                }
                if let Some(quality) = body.get("quality").and_then(Value::as_str) {
                    meta.quality = Some(quality.to_string());
                }
                if let Some(count) = body.get("count").and_then(Value::as_i64) {
                    meta.count = Some(count.clamp(1, 4));
                }
            }
            app.doc.commit(app.now_ms());
        }
    }
    app.ai_start(ui, node_id);
    Reply::ok(&json!({ "started": true, "nodeId": node_id }))
}

fn cancel(app: &mut App, ui: &AppWindow, _id: &str, body: &Value) -> Reply {
    let Some(node_id) = body.get("nodeId").and_then(Value::as_str) else {
        return Reply::error(400, "nodeId is required");
    };
    if app.panel.running.as_deref() == Some(node_id) {
        app.ai_cancel(ui, node_id);
        Reply::ok(&json!({ "cancelled": true, "nodeId": node_id }))
    } else {
        Reply::ok(&json!({ "cancelled": false, "reason": "not running", "nodeId": node_id }))
    }
}

fn screenshot(ui: &AppWindow) -> Reply {
    let buffer = match ui.window().take_snapshot() {
        Ok(buffer) => buffer,
        Err(error) => return Reply::error(500, format!("cannot capture the window: {error}")),
    };
    let width = buffer.width();
    let height = buffer.height();
    let mut pixels = Vec::with_capacity(
        usize::try_from(width)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(height).unwrap_or(0))
            .saturating_mul(4),
    );
    for pixel in buffer.as_slice() {
        pixels.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
    }
    let Some(image) = image::RgbaImage::from_raw(width, height, pixels) else {
        return Reply::error(500, "the screenshot buffer has an unexpected size");
    };
    let mut png = Vec::new();
    if let Err(error) = image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    {
        return Reply::error(500, format!("cannot encode the screenshot: {error}"));
    }
    Reply::png(png)
}

fn next_id(app: &mut App, prefix: &str) -> String {
    app.library.seq = app.library.seq.wrapping_add(1);
    format!("{prefix}-{}-{}", app.now_ms(), app.library.seq)
}
