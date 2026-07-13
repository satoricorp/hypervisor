import { useEffect, useState } from "react";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  getAccess,
  getSettings,
  getTranscript,
  listArchived,
  listHistory,
  setSettings,
  unarchiveSession,
  type AccessRow,
  type AppSettings,
  type ArchivedWire,
  type HistoryRow,
  type TranscriptItem,
} from "../api";
import { CTL_HINT, STATE_META, iconOf } from "../constants";
import { useStore } from "../store";
import { SubagentList } from "./SubagentList";
import { SubTranscript, TranscriptView } from "./Transcript";

function UsagePane() {
  const { state } = useStore();
  const counts: Record<string, number> = {};
  for (const s of state.sessions) {
    const k = s.app || "other";
    counts[k] = (counts[k] ?? 0) + 1;
  }
  const rows = Object.entries(counts).sort((a, b) => b[1] - a[1]);
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Usage</h4>
      <div className="tiles">
        <div className="tile">
          <div className="lbl">Live sessions</div>
          <div className="val">{state.total || state.sessions.length}</div>
          <div className="sub2">on the board right now</div>
        </div>
        {rows.map(([harness, n]) => (
          <div className="tile" key={harness}>
            <div className="lbl">{harness}</div>
            <div className="val">{n}</div>
            <div className="sub2">live</div>
          </div>
        ))}
      </div>
      <p className="footnote">
        cost ledger lands with M6 — no fake dollar numbers. counts are live
        from the session adapters.
      </p>
    </div>
  );
}

function AccessPane() {
  const [rows, setRows] = useState<AccessRow[] | null>(null);
  useEffect(() => {
    void getAccess()
      .then(setRows)
      .catch(() => setRows([]));
  }, []);
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Keys &amp; subscriptions</h4>
      {rows === null ? (
        <div className="listrow">
          <span className="cd">probing…</span>
        </div>
      ) : rows.length === 0 ? (
        <div className="listrow">
          <span className="dim">nothing detected</span>
        </div>
      ) : (
        rows.map((r) => (
          <div className="listrow" key={`${r.label}-${r.kind}`}>
            <span className={`grow ${r.present ? "" : "dim"}`}>{r.label}</span>
            <span className="dim">{r.kind}</span>
            <span className="dim">{r.detail}</span>
            <span className={`tabnum ${r.present ? "st-done" : "dim"}`}>
              {r.present ? "● present" : "○ missing"}
            </span>
          </div>
        ))
      )}
      <p className="footnote">
        presence only — hypervisor never stores key material, never proxies or
        resells tokens. unverifiable rows are omitted, not invented.
      </p>
    </div>
  );
}

function Switch({
  on,
  onToggle,
}: {
  on: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className={`switch ${on ? "on" : ""}`}
      onClick={onToggle}
    >
      <i />
    </button>
  );
}

function RemoteSettings() {
  const [status, setStatus] = useState<{
    serve_cmd: string;
    tailscale_ok: boolean;
    login?: string | null;
    host: string;
    port: number;
  } | null>(null);
  const [imessage, setImessage] = useState<{
    enabled: boolean;
    approvals: boolean;
    fda_ok: boolean;
    detail: string;
  } | null>(null);

  useEffect(() => {
    import("../api")
      .then(({ remoteStatus, imessageStatus }) =>
        Promise.all([remoteStatus(), imessageStatus()]),
      )
      .then(([remote, im]) => {
        setStatus(remote);
        setImessage(im);
      })
      .catch(() =>
        setStatus({
          serve_cmd: "tailscale serve --bg 127.0.0.1:7428",
          tailscale_ok: false,
          host: "127.0.0.1:7428",
          port: 7428,
        }),
      );
  }, []);

  if (!status) {
    return (
      <>
        <h4>Remote</h4>
        <p className="footnote">checking tailscale…</p>
      </>
    );
  }

  return (
    <>
      <h4>Remote</h4>
      <div className="listrow">
        <span>tailscale</span>
        <span className="dim">
          {status.tailscale_ok
            ? `on · ${status.login ?? status.host}`
            : "off (tailscale not detected)"}
        </span>
      </div>
      <div className="listrow">
        <span>serve</span>
        <span className="dim" style={{ fontFamily: "var(--mono)", fontSize: 11 }}>
          {status.serve_cmd}
        </span>
      </div>
      <div className="listrow">
        <span>imessage</span>
        <span className="dim">{imessage?.detail ?? "…"}</span>
      </div>
      <p className="footnote">
        phone page binds 127.0.0.1:{status.port} only — expose with the
        command above. auth via Tailscale-User-Login. no funnel, no yolo.
        imessage needs Full Disk Access for chat.db and Automation for
        Messages on first send.
      </p>
    </>
  );
}

function SettingsPane() {
  const { dispatch } = useStore();
  const [settings, setLocal] = useState<AppSettings | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    void getSettings()
      .then(setLocal)
      .catch((e) => dispatch({ type: "TOAST", label: String(e) }));
    void isAutostartEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(false));
  }, [dispatch]);

  async function patch(next: AppSettings) {
    setLocal(next);
    try {
      const saved = await setSettings(next);
      setLocal(saved);
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
    }
  }

  async function toggleAutostart() {
    try {
      if (autostart) {
        await disableAutostart();
        setAutostart(false);
      } else {
        await enableAutostart();
        setAutostart(true);
      }
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
    }
  }

  if (!settings) {
    return (
      <div className="pane">
        <span className="escnote">esc ↩ session</span>
        <h4>Settings</h4>
        <p className="footnote">loading…</p>
      </div>
    );
  }

  const src = settings.sources;
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <RemoteSettings />
      <h4>iMessage</h4>
      <div className="listrow">
        <span>bridge</span>
        <span className="dim">poll self-chat · grammar</span>
        <Switch
          on={settings.imessage_bridge_enabled}
          onToggle={() =>
            void patch({
              ...settings,
              imessage_bridge_enabled: !settings.imessage_bridge_enabled,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>approvals over imessage</span>
        <span className="dim">off by default · soft identity</span>
        <Switch
          on={settings.imessage_approvals}
          onToggle={() =>
            void patch({
              ...settings,
              imessage_approvals: !settings.imessage_approvals,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>push · done</span>
        <span className="dim">≤1 text / 30s</span>
        <Switch
          on={settings.imessage_push_done}
          onToggle={() =>
            void patch({
              ...settings,
              imessage_push_done: !settings.imessage_push_done,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>push · needs you</span>
        <span className="dim">batched</span>
        <Switch
          on={settings.imessage_push_needs_you}
          onToggle={() =>
            void patch({
              ...settings,
              imessage_push_needs_you: !settings.imessage_push_needs_you,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>push · stalled</span>
        <span className="dim">batched</span>
        <Switch
          on={settings.imessage_push_stalled}
          onToggle={() =>
            void patch({
              ...settings,
              imessage_push_stalled: !settings.imessage_push_stalled,
            })
          }
        />
      </div>
      <p className="footnote">
        self-chat only · bare letter approves when enabled · otherwise
        &ldquo;approvals are disabled over imessage — use the tailnet page&rdquo;.
      </p>
      <h4>Sources</h4>
      {(
        [
          ["claude", "claude code", "hooks + transcripts"],
          ["codex", "codex", "session files"],
          ["opencode", "opencode", "http api"],
          ["cursor", "cursor", "state.vscdb"],
        ] as const
      ).map(([key, label, dim]) => (
        <div className="listrow" key={key}>
          <span>{label}</span>
          <span className="dim">{dim}</span>
          <Switch
            on={src[key]}
            onToggle={() =>
              void patch({
                ...settings,
                sources: { ...src, [key]: !src[key] },
              })
            }
          />
        </div>
      ))}
      <p className="footnote">
        disabled sources are skipped in the sidebar — owned tmux sessions keep
        running; re-enable to see them again.
      </p>
      <h4>General</h4>
      <div className="listrow">
        <span>analytics</span>
        <span className="dim">anonymous feature counts, never content</span>
        <Switch
          on={settings.analytics}
          onToggle={() =>
            void patch({
              ...settings,
              analytics: !settings.analytics,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>tv: pause when a session needs me</span>
        <span className="dim">interrupts the youtube pip</span>
        <Switch
          on={settings.tv_pause_on_needs_you}
          onToggle={() =>
            void patch({
              ...settings,
              tv_pause_on_needs_you: !settings.tv_pause_on_needs_you,
            })
          }
        />
      </div>
      <div className="listrow">
        <span>launch at login</span>
        <span className="dim">macos login item</span>
        <Switch
          on={!!autostart}
          onToggle={() => void toggleAutostart()}
        />
      </div>
      <p className="footnote">
        settings.json in app data · analytics = names/counts only (see tasks/POSTHOG.md) ·
        auto-worktree returns with M4
      </p>
    </div>
  );
}

function formatWhen(mtime: number): string {
  const d = new Date(mtime * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  const now = Date.now();
  const diff = now - d.getTime();
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function HistoryDetail({
  sid,
  title,
  onBack,
}: {
  sid: string;
  title: string;
  onBack: () => void;
}) {
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void getTranscript(sid, 400)
      .then((rows) => {
        if (!cancelled) setItems(rows);
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sid]);

  // Minimal session stub for TranscriptView.
  const stub = {
    app: "",
    model: "",
    title,
    sent: "",
    state: "done" as const,
    ctl: "observe" as const,
    subs: [],
    sid,
  };

  return (
    <div className="pane">
      <span className="escnote">esc ↩ history</span>
      <button type="button" className="archbtn" onClick={onBack}>
        ← history
      </button>
      <h4>{title}</h4>
      <TranscriptView session={stub} items={items} loading={loading} />
      <p className="footnote">read-only · M5 replaces this with sqlite + summaries</p>
    </div>
  );
}

function HistoryPane() {
  const { state, dispatch } = useStore();
  const [rows, setRows] = useState<HistoryRow[] | null>(null);
  const [detail, setDetail] = useState<{ sid: string; title: string } | null>(
    null,
  );
  const q = state.historyFilter.toLowerCase();

  useEffect(() => {
    void listHistory()
      .then(setRows)
      .catch((e) => {
        dispatch({ type: "TOAST", label: String(e) });
        setRows([]);
      });
  }, [dispatch]);

  if (detail) {
    return (
      <HistoryDetail
        sid={detail.sid}
        title={detail.title}
        onBack={() => setDetail(null)}
      />
    );
  }

  const filtered = (rows ?? []).filter(
    (h) =>
      !q ||
      [h.title, h.note, h.model, h.harness, h.sid]
        .join(" ")
        .toLowerCase()
        .includes(q),
  );

  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>History</h4>
      <input
        className="hq"
        id="hq"
        type="text"
        placeholder="search finished sessions…"
        spellCheck={false}
        autoComplete="off"
        value={state.historyFilter}
        onChange={(e) =>
          dispatch({ type: "SET_HISTORY_FILTER", value: e.target.value })
        }
      />
      <div id="hrows">
        {rows === null ? (
          <div className="listrow">
            <span className="cd">loading…</span>
          </div>
        ) : filtered.length === 0 ? (
          <div className="listrow">
            <span className="dim">no older sessions</span>
          </div>
        ) : (
          filtered.map((h) => (
            <div
              key={h.sid}
              className="listrow"
              role="button"
              tabIndex={0}
              style={{ cursor: "pointer" }}
              onClick={() => setDetail({ sid: h.sid, title: h.title })}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  setDetail({ sid: h.sid, title: h.title });
                }
              }}
            >
              <span className="dim" style={{ width: 88, flex: "none" }}>
                {formatWhen(h.mtime)}
              </span>
              <span className="grow">{h.title}</span>
              <span className="dim grow">{h.note}</span>
              {h.model ? <span className="modelchip">{h.model}</span> : null}
              {h.harness ? (
                <span
                  className="hicon"
                  title={h.harness}
                  dangerouslySetInnerHTML={{ __html: iconOf(h.harness) }}
                />
              ) : null}
            </div>
          ))
        )}
      </div>
      <p className="footnote">
        interim — older than the sidebar window + archived tombstones. M5
        replaces this with sqlite + summaries.
      </p>
    </div>
  );
}

function formatArchivedWhen(at: number): string {
  const d = new Date(at * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  const now = Date.now();
  const diff = now - d.getTime();
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function ArchivedPane() {
  const { dispatch } = useStore();
  const [rows, setRows] = useState<ArchivedWire[] | null>(null);

  async function reload() {
    try {
      setRows(await listArchived());
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
      setRows([]);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Archived</h4>
      <div id="archrows">
        {rows === null ? (
          <div className="listrow">
            <span className="cd">loading…</span>
          </div>
        ) : rows.length === 0 ? (
          <div className="listrow">
            <span className="dim">nothing archived</span>
          </div>
        ) : (
          rows.map((r) => (
            <div key={r.sid} className="listrow">
              <span className="grow">{r.title || r.sid}</span>
              <span className="dim">{r.harness || "—"}</span>
              <span className="dim">{formatArchivedWhen(r.archived_at)}</span>
              <button
                type="button"
                className="archbtn"
                onClick={() => {
                  void (async () => {
                    try {
                      await unarchiveSession(r.sid);
                      dispatch({
                        type: "TOAST",
                        label: "unarchived",
                        detail: r.title || r.sid,
                      });
                      await reload();
                    } catch (e) {
                      dispatch({ type: "TOAST", label: String(e) });
                    }
                  })();
                }}
              >
                unarchive
              </button>
            </div>
          ))
        )}
      </div>
      <p className="footnote">
        local tombstones only — transcripts stay on disk · resurfaces on new
        activity
      </p>
    </div>
  );
}

function SessionView() {
  const { state } = useStore();
  const s = state.sessions[state.sel];
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [loading, setLoading] = useState(false);

  // Refresh transcript when selection changes or the selected sid updates.
  const sid = s?.sid;
  const mtime = s?.age; // age ticks; also re-fetch on sessions:update via sid+state
  const activity = s?.tool;
  const lastSent = s?.sent;
  const lastState = s?.state;

  useEffect(() => {
    if (!sid) {
      setItems([]);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void getTranscript(sid, 400)
      .then((rows) => {
        if (!cancelled) setItems(rows);
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sid, mtime, activity, lastSent, lastState, state.total]);

  if (!s) {
    return (
      <div className="dwrap">
        <div className="dhead">
          <h3>no sessions yet</h3>
        </div>
        <p className="dfrom">
          + New Agent or /new to spawn one in hypervisor tmux
        </p>
      </div>
    );
  }

  if (state.subSel >= 0 && s.subs[state.subSel]) {
    const x = s.subs[state.subSel];
    return (
      <div className="dwrap">
        <div className="dhead">
          <span className={`status ${STATE_META[x.state].cls}`}>
            <i className="dot" />
          </span>
          <h3>{x.task || "subagent"}</h3>
          <span className="meta">
            <span className="modelchip">{x.model}</span>
            <span
              className="hicon"
              title={x.target}
              dangerouslySetInnerHTML={{ __html: iconOf(x.target) }}
            />
          </span>
        </div>
        <p className="dfrom">
          ↳ subagent {state.sel + 1}·{state.subSel + 1} of “{s.title}” · h
          steps back up
        </p>
        <SubTranscript sub={x} />
      </div>
    );
  }

  const m = STATE_META[s.state];
  const hint = CTL_HINT[s.ctl] || CTL_HINT.tmux;

  return (
    <div className="dwrap">
      <div className="dhead">
        <span className={`status ${m.cls}`}>
          <i className="dot" />
        </span>
        <h3>{s.title}</h3>
        <span className="meta">
          {s.loop ? <span className="loopchip">↻ loop</span> : null}
          <span
            className={`ctlchip ${s.ctl === "observe" ? "observe" : ""}`}
            title={hint.tip}
          >
            {hint.label}
          </span>
          <span className="modelchip">{s.model}</span>
          <span
            className="hicon"
            title={s.app}
            dangerouslySetInnerHTML={{ __html: iconOf(s.app) }}
          />
        </span>
      </div>
      {s.repo ? (
        <p className="dfrom">
          ⎇ {s.repo} · {s.br || "main"}
          {s.wt ? ` · worktree ${s.wt}` : ""}
        </p>
      ) : null}
      <SubagentList />
      <TranscriptView session={s} items={items} loading={loading} />
    </div>
  );
}

export function MainPane() {
  const { state } = useStore();
  let body;
  if (state.view === "usage") body = <UsagePane />;
  else if (state.view === "access") body = <AccessPane />;
  else if (state.view === "settings") body = <SettingsPane />;
  else if (state.view === "history") body = <HistoryPane />;
  else if (state.view === "archived") body = <ArchivedPane />;
  else body = <SessionView />;
  return <main id="main">{body}</main>;
}
