import type { Session, SessionState } from "./types";

export const STATE_META: Record<
  SessionState,
  { cls: string; color: string }
> = {
  working: { cls: "st-working", color: "var(--busy)" },
  done: { cls: "st-done", color: "var(--ok)" },
  input: { cls: "st-input", color: "var(--err)" },
  error: { cls: "st-error", color: "var(--err)" },
};

export const CTL_HINT: Record<
  string,
  { label: string; tip: string }
> = {
  tmux: {
    label: "⏻ runs in background",
    tip: "detached in hypervisor tmux — keeps running when Hypervisor is closed, and stays awake while working (lid closed on AC power)",
  },
  native: {
    label: "follow-only",
    tip: "follow-only — Hypervisor mirrors this session but can’t drive it",
  },
  api: {
    label: "follow-only",
    tip: "follow-only — Hypervisor mirrors this session but can’t drive it",
  },
  watch: {
    label: "follow-only",
    tip: "follow-only — Hypervisor mirrors this session but can’t drive it",
  },
  observe: {
    label: "not controlled",
    tip: "Hypervisor isn’t driving this yet — ⏎ to take it over",
  },
};

export const ICON: Record<string, string> = {
  "claude code":
    '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3v5M12 16v5M3 12h5M16 12h5M5.6 5.6l3.6 3.6M14.8 14.8l3.6 3.6M18.4 5.6l-3.6 3.6M9.2 14.8l-3.6 3.6"/></svg>',
  "claude.ai":
    '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M12 7.5v3M12 13.5v3M7.5 12h3M13.5 12h3M8.9 8.9l2 2M13.1 13.1l2 2M15.1 8.9l-2 2M10.9 13.1l-2 2"/></svg>',
  codex:
    '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><ellipse cx="12" cy="12" rx="9" ry="3.8"/><ellipse cx="12" cy="12" rx="9" ry="3.8" transform="rotate(60 12 12)"/><ellipse cx="12" cy="12" rx="9" ry="3.8" transform="rotate(120 12 12)"/></svg>',
  cursor:
    '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M12 2.5l8.5 4.75v9.5L12 21.5l-8.5-4.75v-9.5z"/><path d="M12 21.5V12M12 12L3.5 7.25M12 12l8.5-4.75"/></svg>',
  opencode:
    '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15H17"/></svg>',
};

export function iconOf(n: string): string {
  return ICON[n] || (n === "claude" ? ICON["claude code"] : "");
}

/** Transcript from real adapter fields only (full history is M5). */
export function buildLog(s: Session) {
  if (s.log && s.log.length) return s.log;
  const L: { k: "you" | "agent" | "tool"; t: string }[] = [];
  if (s.title) L.push({ k: "you", t: s.title });
  if (s.sent && s.sent !== "—" && s.sent !== s.title) {
    L.push({ k: "you", t: s.sent });
  }
  return L;
}

export function ensureLog(s: Session) {
  if (!s.log) s.log = buildLog(s);
  return s.log;
}
