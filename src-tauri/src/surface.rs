//! M7: the macOS surface.
//!
//! A menu-bar tray with a state-colored dot + needs-you count, a dock badge,
//! ⌥Space to summon the window, and a notification per newly-pending approval.
//! Presence + push only — approvals still flow through the existing tauri
//! commands and the shared board state. Consumes `grammar.rs` labels for copy.

use crate::events::AppState;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

const TRAY_ID: &str = "hv-tray";

/// sids we've already fired a needs-you notification for (so a pending approval
/// notifies once, not every 2s tick). Pruned when the approval resolves.
fn notified() -> &'static Mutex<HashSet<String>> {
    static N: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    N.get_or_init(|| Mutex::new(HashSet::new()))
}

#[derive(Clone, Copy, PartialEq)]
enum Dot {
    Green,
    Yellow,
    Red,
}

/// An 18×18 RGBA filled circle for the menu bar (non-template, so it stays
/// colored): red = needs you, yellow = working, green = all clear.
fn dot_image(d: Dot) -> Image<'static> {
    let (r, g, b) = match d {
        Dot::Green => (70u8, 214, 140),
        Dot::Yellow => (226, 163, 62),
        Dot::Red => (229, 84, 75),
    };
    let n: usize = 18;
    let (cx, cy, rad) = (8.5f64, 8.5, 5.4);
    let mut buf = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let a: u8 = if dist <= rad {
                255
            } else if dist <= rad + 1.2 {
                ((rad + 1.2 - dist) / 1.2 * 255.0) as u8
            } else {
                0
            };
            let i = (y * n + x) * 4;
            buf[i] = r;
            buf[i + 1] = g;
            buf[i + 2] = b;
            buf[i + 3] = a;
        }
    }
    Image::new_owned(buf, n as u32, n as u32)
}

struct Pending {
    title: String,
    approval: String,
    sid: String,
}

/// Build the tray once at startup with an empty state.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &[])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(dot_image(Dot::Green))
        .icon_as_template(false)
        .tooltip("hypervisor")
        .menu(&menu)
        .on_menu_event(on_menu)
        .build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle, pending: &[Pending]) -> tauri::Result<Menu<Wry>> {
    let header_text = if pending.is_empty() {
        "hypervisor — all clear".to_string()
    } else {
        format!("hypervisor — {} need you", pending.len())
    };
    let header = MenuItem::with_id(app, "hdr", header_text, false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "show hypervisor", true, Some("Alt+Space"))?;
    let quit = MenuItem::with_id(app, "quit", "quit hypervisor", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<Wry>>> = Vec::new();
    items.push(Box::new(header));
    items.push(Box::new(sep1));
    for p in pending {
        let label = format!("▸ {}  —  {}", trunc(&p.title, 30), trunc(&p.approval, 38));
        items.push(Box::new(MenuItem::with_id(
            app,
            format!("focus:{}", p.sid),
            label,
            true,
            None::<&str>,
        )?));
    }
    items.push(Box::new(show));
    items.push(Box::new(sep2));
    items.push(Box::new(quit));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().0.clone();
    if id == "quit" {
        app.exit(0);
    } else if id == "show" || id.starts_with("focus:") {
        show_window(app);
    }
}

/// Reveal + focus the main window (it hides on close rather than quitting).
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Called after every snapshot emit: recolor the dot, set the count + dock
/// badge, rebuild the menu, and notify each newly-pending approval.
pub fn refresh(app: &AppHandle, state: &AppState) {
    let (pending, working) = {
        let snap = state
            .snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let pending: Vec<Pending> = snap
            .iter()
            .filter(|s| s.state == "needs_you")
            .map(|s| Pending {
                title: s.title.clone(),
                approval: s.approval.clone().unwrap_or_default(),
                sid: s.sid.clone(),
            })
            .collect();
        let working = snap.iter().any(|s| s.state == "working");
        (pending, working)
    };

    let n = pending.len();
    let dot = if n > 0 {
        Dot::Red
    } else if working {
        Dot::Yellow
    } else {
        Dot::Green
    };

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_icon(Some(dot_image(dot)));
        let _ = tray.set_title(Some(if n > 0 { n.to_string() } else { String::new() }));
        if let Ok(menu) = build_menu(app, &pending) {
            let _ = tray.set_menu(Some(menu));
        }
    }

    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_badge_count(if n > 0 { Some(n as i64) } else { None });
    }

    // One notification per newly-pending approval; prune resolved ones.
    let live: HashSet<String> = pending.iter().map(|p| p.sid.clone()).collect();
    let mut seen = notified().lock().unwrap_or_else(|p| p.into_inner());
    seen.retain(|sid| live.contains(sid));
    for p in &pending {
        if seen.insert(p.sid.clone()) {
            let body = if p.approval.is_empty() {
                "a session needs your input".to_string()
            } else {
                p.approval.clone()
            };
            let _ = app
                .notification()
                .builder()
                .title(format!("needs you — {}", trunc(&p.title, 40)))
                .body(body)
                .show();
        }
    }
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}
