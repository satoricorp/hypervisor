/** Map Rust SessionWire (snake_case) → UI Session. */

import type { ControlTier, LogEntry, Session, SessionState } from "./types";

export interface SessionWire {
  harness: string;
  sid: string;
  title: string;
  model: string;
  cwd: string;
  branch: string;
  last_user: string;
  last_assistant: string;
  activity: string;
  mtime: number;
  state: string;
  age: string;
  repo: string;
  src: string;
  sidechains: number;
  control: string;
}

function mapState(state: string): SessionState {
  if (state === "working") return "working";
  if (state === "stalled") return "error";
  return "done";
}

function mapControl(control: string): ControlTier {
  if (control === "tmux") return "tmux";
  if (control === "watch") return "watch";
  if (control === "api") return "native";
  return "observe";
}

export function wireToSession(w: SessionWire): Session {
  const title = w.title || w.sid || "(untitled)";
  const sent = w.last_user || "—";
  const state = mapState(w.state);
  const log: LogEntry[] = [];
  if (w.title) log.push({ k: "you", t: w.title });
  if (w.last_user && w.last_user !== w.title) {
    log.push({ k: "you", t: w.last_user });
  }
  if (w.last_assistant) log.push({ k: "agent", t: w.last_assistant });

  const s: Session = {
    app: w.harness,
    model: w.model || "—",
    title,
    sent,
    state,
    ctl: mapControl(w.control),
    repo: w.repo && w.repo !== "-" ? w.repo : undefined,
    br: w.branch || undefined,
    cwd: w.cwd || undefined,
    sid: w.sid,
    age: w.age,
    sidechains: w.sidechains || 0,
    subs: [],
    log,
  };

  if (state === "working" && w.activity) {
    // activity is e.g. `Edit(src/auth/middleware.ts)` — split for now-block
    const m = w.activity.match(/^([A-Za-z]+)\((.*)\)$/);
    if (m) {
      s.tool = m[1];
      s.toolArg = m[2];
    } else {
      s.tool = "Tool";
      s.toolArg = w.activity;
    }
  } else if (state === "done" && w.last_assistant) {
    s.result = w.last_assistant;
  } else if (state === "error") {
    s.fail = "stalled — waiting on a reply that never came";
  }

  return s;
}
