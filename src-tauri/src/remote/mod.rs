//! M8a — tailnet phone triage page.
//!
//! // DECISION: tiny_http over axum — no tokio/tower stack; SSE is a plain
//! keep-alive writer per connection. Bind is hardcoded to 127.0.0.1 (no
//! config knob). Exposure is exclusively `tailscale serve`.
//!
//! // DECISION: keep-awake via managed `caffeinate -dims` child — no IOKit
//! binding for v1; release after 60s with no owned session working.

mod keepawake;
pub mod imessage;
mod tailscale;

use crate::approvals::ToastEvent;
use crate::events::{self, AppState, SessionsUpdate};
use crate::grammar::{self, Action, BoardRow};
use keepawake::KeepAwake;
use serde::Serialize;
use serde_json::json;
use std::io::{self, Write};
use std::net::SocketAddrV4;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// Fixed loopback port. Not configurable — Settings shows this exact value.
pub const PORT: u16 = 7428;
const BIND: &str = "127.0.0.1";

const PAGE: &str = include_str!("page.html");
const FONT: &[u8] = include_bytes!("Xer0-Regular.otf");

/// Broadcast bus for SSE clients — gen bumps on every sessions:update.
pub struct SseBus {
    inner: Mutex<(u64, String)>,
    cv: Condvar,
}

impl SseBus {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new((0, String::from("{\"sessions\":[],\"total\":0}"))),
            cv: Condvar::new(),
        }
    }

    pub fn publish(&self, json: String) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.0 = g.0.wrapping_add(1);
        g.1 = json;
        self.cv.notify_all();
    }

    pub(crate) fn snapshot(&self) -> (u64, String) {
        let g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        (g.0, g.1.clone())
    }

    pub(crate) fn wait_after(&self, after: u64, timeout: Duration) -> Option<(u64, String)> {
        let g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let (g, _) = self
            .cv
            .wait_timeout_while(g, timeout, |x| x.0 == after)
            .unwrap_or_else(|e| e.into_inner());
        if g.0 == after {
            None
        } else {
            Some((g.0, g.1.clone()))
        }
    }
}

#[derive(Clone)]
struct RemoteCfg {
    /// Expected `Tailscale-User-Login` value (None → reject all unless HV_DEV).
    login: Option<String>,
    /// MagicDNS / hostname for the page header.
    host: String,
    detected: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct RemoteStatus {
    pub port: u16,
    pub bind: String,
    pub serve_cmd: String,
    pub tailscale_ok: bool,
    pub login: Option<String>,
    pub host: String,
    pub dev_bypass: bool,
}

fn clock() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

fn header_eq(req: &Request, name: &str) -> Option<String> {
    for h in req.headers() {
        if h.field.as_str().as_str().eq_ignore_ascii_case(name) {
            return Some(h.value.as_str().to_string());
        }
    }
    None
}

fn dev_bypass() -> bool {
    matches!(std::env::var("HV_DEV").as_deref(), Ok("1"))
}

fn auth_ok(req: &Request, cfg: &RemoteCfg) -> Result<String, String> {
    if dev_bypass() {
        return Ok(cfg
            .login
            .clone()
            .unwrap_or_else(|| "dev@localhost".into()));
    }
    let Some(expected) = cfg.login.as_ref() else {
        return Err(
            "tailscale not detected — remote auth has no login to match. \
             Start Tailscale, or set HV_DEV=1 for localhost-only testing."
                .into(),
        );
    };
    match header_eq(req, "Tailscale-User-Login") {
        Some(ref got) if got == expected => Ok(got.clone()),
        Some(got) => Err(format!(
            "Tailscale-User-Login mismatch (got {got}, expected {expected})"
        )),
        None => Err(
            "missing Tailscale-User-Login — connect via `tailscale serve`, \
             not direct localhost curl"
                .into(),
        ),
    }
}

fn respond_text(req: Request, code: u16, body: &str, ctype: &str) {
    let response = Response::from_string(body.to_string())
        .with_status_code(StatusCode(code))
        .with_header(Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap());
    let _ = req.respond(response);
}

fn respond_json(req: Request, code: u16, value: &serde_json::Value) {
    respond_text(
        req,
        code,
        &value.to_string(),
        "application/json; charset=utf-8",
    );
}

fn respond_bytes(req: Request, code: u16, body: &[u8], ctype: &str) {
    let response = Response::from_data(body.to_vec())
        .with_status_code(StatusCode(code))
        .with_header(Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap());
    let _ = req.respond(response);
}

fn forbidden(req: Request, msg: &str) {
    respond_text(req, 403, msg, "text/plain; charset=utf-8");
}

pub(crate) fn board_from_wire(sessions: &[crate::events::SessionWire]) -> Vec<BoardRow> {
    sessions
        .iter()
        .map(|s| BoardRow {
            n: s.n,
            sid: s.sid.clone(),
            title: if s.title.is_empty() {
                s.sid.clone()
            } else {
                s.title.clone()
            },
            state: s.state.clone(),
            approval: s.approval.clone(),
            letter: s
                .letter
                .as_ref()
                .and_then(|l| l.chars().next())
                .map(|c| c.to_ascii_uppercase()),
        })
        .collect()
}

fn title_of(rows: &[BoardRow], sid: &str) -> String {
    rows.iter()
        .find(|r| r.sid == sid)
        .map(|r| r.title.clone())
        .unwrap_or_else(|| sid.to_string())
}

fn toast_remote(app: &AppHandle, action: &str, login: &str) {
    let _ = app.emit(
        "toast",
        &ToastEvent {
            label: format!("{action} via remote · {login} · {}", clock()),
            detail: None,
        },
    );
    // TODO(M5): history line — `{action} via remote · {login} · {time}`
}

pub(crate) fn execute_action(
    app: &AppHandle,
    state: &AppState,
    action: Action,
    rows: &[BoardRow],
    login: &str,
) -> Result<String, String> {
    match action {
        Action::PrintStatus => Ok(grammar::format_status(rows)),
        Action::Help => Ok(grammar::HELP.to_string()),
        Action::Err(e) => Err(e),
        Action::Approve { sid, letter } => {
            events::approve_sid(app, state, &sid)?;
            let n = rows
                .iter()
                .find(|r| r.sid == sid)
                .map(|r| r.n)
                .unwrap_or(0);
            toast_remote(app, "approved", login);
            Ok(grammar::echo_approved(letter, n, &title_of(rows, &sid)))
        }
        Action::Deny { sid, n, guidance } => {
            events::deny_sid(app, state, &sid, &guidance)?;
            toast_remote(app, "denied", login);
            Ok(grammar::echo_denied(n, &title_of(rows, &sid)))
        }
        Action::Prompt { sid, n, text } => {
            events::prompt_sid(state, &sid, &text)?;
            toast_remote(app, "prompted", login);
            Ok(grammar::echo_sent(n, &title_of(rows, &sid)))
        }
        Action::Nudge { sid, n } => {
            events::prompt_sid(state, &sid, "continue")?;
            toast_remote(app, "nudged", login);
            Ok(grammar::echo_sent(n, &title_of(rows, &sid)))
        }
    }
}

pub(crate) fn run_command(
    app: &AppHandle,
    state: &Arc<AppState>,
    text: &str,
    login: &str,
) -> Result<String, String> {
    let update = events::current_sessions(state);
    let rows = board_from_wire(&update.sessions);
    let ids = events::ids_snapshot(state);
    let cmd = grammar::parse(text);
    let action = grammar::plan(&cmd, &rows, &ids);
    execute_action(app, state, action, &rows, login)
}

fn read_json_body(req: &mut Request) -> Result<serde_json::Value, String> {
    let mut buf = String::new();
    req.as_reader()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&buf).map_err(|e| e.to_string())
}

fn handle_sse(request: Request, bus: Arc<SseBus>, initial: SessionsUpdate) -> io::Result<()> {
    let mut stream = request.into_writer();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Connection: keep-alive\r\n\
         \r\n"
    )?;
    let init = serde_json::to_string(&initial).unwrap_or_else(|_| "{}".into());
    write!(stream, "event: sessions\ndata: {init}\n\n")?;
    stream.flush()?;

    let (mut gen, _) = bus.snapshot();
    loop {
        match bus.wait_after(gen, Duration::from_secs(25)) {
            Some((g, payload)) => {
                gen = g;
                write!(stream, "event: sessions\ndata: {payload}\n\n")?;
                stream.flush()?;
            }
            None => {
                write!(stream, ": ping\n\n")?;
                stream.flush()?;
            }
        }
    }
}

fn handle(
    mut request: Request,
    app: AppHandle,
    state: Arc<AppState>,
    bus: Arc<SseBus>,
    cfg: RemoteCfg,
) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or(&url);

    if path == "/" || path == "/index.html" {
        if let Err(msg) = auth_ok(&request, &cfg) {
            forbidden(request, &msg);
            return;
        }
        let html = PAGE
            .replace("{{HOST}}", &cfg.host)
            .replace("{{LOGIN}}", cfg.login.as_deref().unwrap_or("—"));
        respond_text(request, 200, &html, "text/html; charset=utf-8");
        return;
    }
    if path == "/Xer0-Regular.otf" {
        if let Err(msg) = auth_ok(&request, &cfg) {
            forbidden(request, &msg);
            return;
        }
        respond_bytes(request, 200, FONT, "font/otf");
        return;
    }

    if !path.starts_with("/api/") {
        respond_text(request, 404, "not found", "text/plain");
        return;
    }

    let login = match auth_ok(&request, &cfg) {
        Ok(l) => l,
        Err(msg) => {
            forbidden(request, &msg);
            return;
        }
    };

    match (method, path) {
        (Method::Get, "/api/sessions") => {
            let update = events::current_sessions(&state);
            respond_json(
                request,
                200,
                &serde_json::to_value(update).unwrap_or(json!({})),
            );
        }
        (Method::Get, "/api/events") => {
            let update = events::current_sessions(&state);
            if let Ok(s) = serde_json::to_string(&update) {
                bus.publish(s);
            }
            if let Err(e) = handle_sse(request, bus, update) {
                eprintln!("[remote] sse ended: {e}");
            }
        }
        (Method::Post, "/api/command") => {
            let body = match read_json_body(&mut request) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(request, 400, &json!({"ok": false, "error": e}));
                    return;
                }
            };
            let text = body
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match run_command(&app, &state, &text, &login) {
                Ok(reply) => respond_json(request, 200, &json!({"ok": true, "reply": reply})),
                Err(e) => {
                    respond_json(request, 200, &json!({"ok": false, "reply": e, "error": e}))
                }
            }
        }
        (Method::Post, "/api/approve") => {
            let body = match read_json_body(&mut request) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(request, 400, &json!({"ok": false, "error": e}));
                    return;
                }
            };
            let letter = body
                .get("letter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or('?');
            match run_command(&app, &state, &letter.to_string(), &login) {
                Ok(reply) => respond_json(request, 200, &json!({"ok": true, "reply": reply})),
                Err(e) => {
                    respond_json(request, 200, &json!({"ok": false, "reply": e, "error": e}))
                }
            }
        }
        (Method::Post, "/api/deny") => {
            let body = match read_json_body(&mut request) {
                Ok(v) => v,
                Err(e) => {
                    respond_json(request, 400, &json!({"ok": false, "error": e}));
                    return;
                }
            };
            let letter = body
                .get("letter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let guidance = body
                .get("guidance")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let update = events::current_sessions(&state);
            let rows = board_from_wire(&update.sessions);
            let ids = events::ids_snapshot(&state);
            let letter_c = letter
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or('?');
            let action = grammar::plan(&grammar::Command::ApproveLetter(letter_c), &rows, &ids);
            let result = match action {
                Action::Approve { sid, .. } => {
                    let n = ids.number_of(&sid).unwrap_or(0);
                    run_command(&app, &state, &format!("{n}: {guidance}"), &login)
                }
                Action::Err(e) => Err(e),
                _ => Err(format!("no pending approval for letter {letter_c}")),
            };
            match result {
                Ok(reply) => respond_json(request, 200, &json!({"ok": true, "reply": reply})),
                Err(e) => {
                    respond_json(request, 200, &json!({"ok": false, "reply": e, "error": e}))
                }
            }
        }
        _ => {
            // Explicitly no /api/yolo.
            respond_text(request, 404, "not found", "text/plain");
        }
    }
}

/// Publish a sessions:update JSON blob to all SSE clients.
pub fn broadcast_sessions(bus: &SseBus, update: &SessionsUpdate) {
    if let Ok(s) = serde_json::to_string(update) {
        bus.publish(s);
    }
}

fn build_cfg() -> RemoteCfg {
    match tailscale::detect() {
        Some(info) => RemoteCfg {
            login: Some(info.login),
            host: info.dns_name.trim_end_matches('.').to_string(),
            detected: true,
        },
        None => RemoteCfg {
            login: None,
            host: format!("{BIND}:{PORT}"),
            detected: false,
        },
    }
}

/// Start the loopback HTTP server + keep-awake watcher. Call once from setup.
pub fn start(app: AppHandle, state: Arc<AppState>) {
    // M8b: iMessage bridge (polls only while settings.imessage_bridge_enabled).
    imessage::start(app.clone(), Arc::clone(&state));

    let bus = Arc::clone(&state.remote_bus);
    let cfg = build_cfg();
    if cfg.detected {
        eprintln!(
            "[remote] listening on {BIND}:{PORT} · login={} · host={}",
            cfg.login.as_deref().unwrap_or("?"),
            cfg.host
        );
    } else {
        eprintln!(
            "[remote] listening on {BIND}:{PORT} · tailscale not detected \
             (auth will 403 unless HV_DEV=1)"
        );
    }

    {
        let update = events::current_sessions(&state);
        broadcast_sessions(&bus, &update);
    }

    {
        let st = Arc::clone(&state);
        thread::spawn(move || {
            let mut ka = KeepAwake::new();
            let mut idle_since: Option<Instant> = None;
            loop {
                let working = events::any_owned_working(&st);
                if working {
                    ka.hold();
                    idle_since = None;
                } else {
                    match idle_since {
                        None => idle_since = Some(Instant::now()),
                        Some(t) if t.elapsed() >= Duration::from_secs(60) => {
                            ka.release();
                        }
                        Some(_) => {}
                    }
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
    }

    thread::spawn(move || {
        // Refuse any non-loopback bind by construction — SocketAddrV4 only.
        let addr = SocketAddrV4::new(BIND.parse().expect("127.0.0.1"), PORT);
        let server = match Server::http(addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[remote] failed to bind {addr}: {e}");
                return;
            }
        };
        loop {
            let request = match server.recv() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[remote] recv: {e}");
                    continue;
                }
            };
            let app = app.clone();
            let state = Arc::clone(&state);
            let bus = Arc::clone(&bus);
            let cfg = cfg.clone();
            thread::spawn(move || handle(request, app, state, bus, cfg));
        }
    });
}

#[tauri::command]
pub fn remote_status(state: State<'_, Arc<AppState>>) -> RemoteStatus {
    let _ = state;
    let cfg = build_cfg();
    RemoteStatus {
        port: PORT,
        bind: BIND.into(),
        serve_cmd: format!("tailscale serve --bg {BIND}:{PORT}"),
        tailscale_ok: cfg.detected,
        login: cfg.login,
        host: cfg.host,
        dev_bypass: dev_bypass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_is_loopback_only() {
        assert_eq!(BIND, "127.0.0.1");
        let addr = SocketAddrV4::new(BIND.parse().unwrap(), PORT);
        assert!(addr.ip().is_loopback());
    }

    #[test]
    fn sse_bus_notifies() {
        let bus = SseBus::new();
        let (g0, _) = bus.snapshot();
        bus.publish("{\"ok\":1}".into());
        let (g1, p) = bus.snapshot();
        assert!(g1 > g0);
        assert!(p.contains("ok"));
    }
}
