//! TV — design-review prototype (spec: design/tv.md). Not a milestone.
//! A PiP-style satellite WebviewWindow on youtube.com that pauses itself and
//! shows an interrupt strip when a session needs attention. Built so the
//! interaction can be reviewed in the real app; the eventual milestone
//! replaces this with the full treatment (corner snapping, cooldown, Tab
//! routing into approvals once M3 exists).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "tv";

/// Open the PiP player if closed, close it if open. Returns the new state.
#[tauri::command]
pub fn toggle_tv(app: AppHandle) -> Result<bool, String> {
    if let Some(w) = app.get_webview_window(LABEL) {
        w.close().map_err(|e| e.to_string())?;
        return Ok(false);
    }
    WebviewWindowBuilder::new(
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
    .build()
    .map_err(|e| e.to_string())?;
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
