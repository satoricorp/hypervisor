//! TV — design-review prototype (spec: design/tv.md). Not a milestone.
//! A PiP-style satellite WebviewWindow on youtube.com that pauses itself and
//! shows an interrupt strip when a session needs attention. Built so the
//! interaction can be reviewed in the real app; the eventual milestone
//! replaces this with the full treatment (corner snapping, cooldown, Tab
//! routing into approvals once M3 exists).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "tv";

/// Runs on every navigation in the tv window. On watch pages, flattens
/// YouTube into a bare player that fills the window (native controls appear
/// on hover); browse pages stay normal so you can pick a video. SPA-aware
/// via yt-navigate-finish. Best-effort: DOM drift degrades to plain youtube.
const TV_INIT_JS: &str = r#"
(() => {
  if (window.__hvtv) return; window.__hvtv = true;
  const FILL = `
    ytd-masthead, #masthead-container { display: none !important; }
    html, body { overflow: hidden !important; background: #000 !important; }
    #movie_player {
      position: fixed !important; inset: 0 !important;
      width: 100vw !important; height: 100vh !important;
      z-index: 999999 !important; background: #000 !important;
    }
    #movie_player video { object-fit: contain !important; }
  `;
  const style = document.createElement('style');
  style.id = 'hv-tv-style';
  const apply = () => {
    style.textContent = location.pathname === '/watch' ? FILL : '';
    // keep the video sized to the window in fill mode
    if (location.pathname === '/watch') {
      const v = document.querySelector('#movie_player video');
      if (v) { v.style.width = '100vw'; v.style.height = '100vh'; v.style.left = '0'; v.style.top = '0'; }
      window.dispatchEvent(new Event('resize'));
    }
  };
  const attach = setInterval(() => {
    if (document.head) {
      document.head.appendChild(style);
      apply();
      clearInterval(attach);
    }
  }, 120);
  document.addEventListener('yt-navigate-finish', () => setTimeout(apply, 250));
  window.addEventListener('resize', () => setTimeout(apply, 100));
})();
"#;

/// macOS: chromeless windows aren't draggable by default — flip the native
/// NSWindow flag so the whole video surface drags the window (real PiP feel).
#[cfg(target_os = "macos")]
fn make_drag_anywhere(w: &tauri::WebviewWindow) {
    use objc::{msg_send, sel, sel_impl};
    if let Ok(ns) = w.ns_window() {
        let ns = ns as *mut objc::runtime::Object;
        unsafe {
            let _: () = msg_send![ns, setMovableByWindowBackground: true];
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn make_drag_anywhere(_w: &tauri::WebviewWindow) {}

/// Open the PiP player if closed, close it if open. Returns the new state.
#[tauri::command]
pub fn toggle_tv(app: AppHandle) -> Result<bool, String> {
    if let Some(w) = app.get_webview_window(LABEL) {
        w.close().map_err(|e| e.to_string())?;
        return Ok(false);
    }
    let w = WebviewWindowBuilder::new(
        &app,
        LABEL,
        WebviewUrl::External(
            "https://www.youtube.com"
                .parse()
                .map_err(|e| format!("{e}"))?,
        ),
    )
    .title("hypervisor tv")
    .inner_size(440.0, 270.0)
    .decorations(false)
    .always_on_top(true)
    .initialization_script(TV_INIT_JS)
    .build()
    .map_err(|e| e.to_string())?;
    make_drag_anywhere(&w);
    Ok(true)
}

/// Pause the video and show/refresh the interrupt strip inside the tv window.
/// Best-effort: if YouTube's DOM shifts, the strip still shows (design/tv.md).
#[tauri::command]
pub fn tv_interrupt(app: AppHandle, title: String, detail: String) -> Result<(), String> {
    let Some(w) = app.get_webview_window(LABEL) else {
        return Ok(()); // no tv, no interrupt — never an error
    };
    let text = serde_json::to_string(&format!("⏸ {title} — {detail}"))
        .map_err(|e| e.to_string())?;
    let js = format!(
        r#"(() => {{
  try {{ document.querySelector('video') && document.querySelector('video').pause(); }} catch (_e) {{}}
  let s = document.getElementById('hv-interrupt');
  if (!s) {{
    s = document.createElement('div');
    s.id = 'hv-interrupt';
    s.style.cssText = 'position:fixed;top:8px;left:8px;right:8px;z-index:2147483647;background:rgba(12,15,19,.96);border:1px solid rgba(229,84,75,.55);border-left:3px solid #e5544b;color:#dee3e8;font:11px ui-monospace,Menlo,monospace;padding:9px 11px;border-radius:2px;display:flex;gap:10px;align-items:center';
    const t = document.createElement('span');
    t.id = 'hv-interrupt-text';
    t.style.cssText = 'flex:1;min-width:0;line-height:1.4';
    const b = document.createElement('button');
    b.textContent = 'resume';
    b.style.cssText = 'font:inherit;color:#fff;background:rgba(20,22,26,.9);border:1px solid rgba(255,255,255,.2);border-radius:2px;padding:3px 10px;cursor:pointer;flex:none';
    b.onclick = () => {{
      s.remove();
      try {{ document.querySelector('video') && document.querySelector('video').play(); }} catch (_e) {{}}
    }};
    s.append(t, b);
    document.body.appendChild(s);
  }}
  document.getElementById('hv-interrupt-text').textContent = {text};
}})();"#
    );
    w.eval(&js).map_err(|e| e.to_string())
}
