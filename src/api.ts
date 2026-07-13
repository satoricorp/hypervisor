import { invoke } from "@tauri-apps/api/core";
import type { SessionWire } from "./wire";

export interface SessionsUpdate {
  sessions: SessionWire[];
  total: number;
}

export async function listSessions(): Promise<SessionsUpdate> {
  return invoke<SessionsUpdate>("list_sessions");
}

export async function spawnSession(
  harness: string,
  model: string,
  cwd?: string | null,
  via: "new" | "subagents" = "new",
  worktree?: boolean | null,
): Promise<string> {
  return invoke<string>("spawn_session", {
    harness,
    model,
    cwd: cwd ?? null,
    via,
    worktree: worktree ?? null,
  });
}

export async function sendPrompt(sid: string, text: string): Promise<void> {
  return invoke("send_prompt", { sid, text });
}

export async function adoptSession(sid: string): Promise<string> {
  return invoke<string>("adopt_session", { sid });
}

export async function approveSession(sid: string): Promise<void> {
  return invoke("approve_session", { sid });
}

export async function denySession(
  sid: string,
  guidance: string,
): Promise<void> {
  return invoke("deny_session", { sid, guidance });
}

export async function setYolo(on: boolean): Promise<void> {
  return invoke("set_yolo", { on });
}

export async function getYolo(): Promise<boolean> {
  return invoke<boolean>("get_yolo");
}

export interface RemoteStatus {
  port: number;
  bind: string;
  serve_cmd: string;
  tailscale_ok: boolean;
  login?: string | null;
  host: string;
  dev_bypass: boolean;
}

export async function remoteStatus(): Promise<RemoteStatus> {
  return invoke<RemoteStatus>("remote_status");
}

/** Wait up to 15s for a newly owned (tmux) sid that wasn't in `before`. */
export async function waitForOwnedSid(
  before: Set<string>,
  timeoutMs = 15_000,
): Promise<string | null> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const update = await listSessions();
    const found = update.sessions.find(
      (s) => s.control === "tmux" && s.sid && !before.has(s.sid),
    );
    if (found) return found.sid;
    await new Promise((r) => setTimeout(r, 500));
  }
  return null;
}

export async function toggleTv(): Promise<boolean> {
  return invoke<boolean>("toggle_tv");
}

export interface ArchivedWire {
  sid: string;
  title: string;
  harness: string;
  archived_at: number;
}

export async function archiveSession(sid: string): Promise<string> {
  return invoke<string>("archive_session", { sid });
}

export async function unarchiveSession(sid: string): Promise<void> {
  return invoke("unarchive_session", { sid });
}

export async function listArchived(): Promise<ArchivedWire[]> {
  return invoke<ArchivedWire[]>("list_archived");
}

export async function archiveIdle(): Promise<number> {
  return invoke<number>("archive_idle");
}

export type TranscriptItem =
  | { kind: "user"; text: string }
  | { kind: "assistant"; text: string }
  | { kind: "thinking"; text: string }
  | {
      kind: "tool";
      id: string;
      name: string;
      summary: string;
      input: string;
      result: string | null;
      is_error: boolean;
    };

export async function getTranscript(
  sid: string,
  limit?: number,
): Promise<TranscriptItem[]> {
  return invoke<TranscriptItem[]>("get_transcript", {
    sid,
    limit: limit ?? null,
  });
}

export async function renameSession(
  sid: string,
  title: string,
): Promise<string> {
  return invoke<string>("rename_session", { sid, title });
}

export async function killSession(sid: string): Promise<string> {
  return invoke<string>("kill_session", { sid });
}

export async function compactSession(sid: string): Promise<string> {
  return invoke<string>("compact_session", { sid });
}

export interface BroadcastResult {
  sid: string;
  title: string;
  ok: boolean;
  detail: string;
}

export async function broadcastPrompt(
  text: string,
): Promise<BroadcastResult[]> {
  return invoke<BroadcastResult[]>("broadcast_prompt", { text });
}

export interface SourceToggles {
  claude: boolean;
  codex: boolean;
  cursor: boolean;
  opencode: boolean;
}

export interface AppSettings {
  tv_pause_on_needs_you: boolean;
  sources: SourceToggles;
  imessage_bridge_enabled: boolean;
  imessage_approvals: boolean;
  imessage_push_done: boolean;
  imessage_push_needs_you: boolean;
  imessage_push_stalled: boolean;
  analytics: boolean;
  distinct_id: string;
  analytics_notice_shown: boolean;
  auto_worktree: boolean;
}

export interface UsageTokens {
  input: number;
  cache_write: number;
  cache_read: number;
  output: number;
}

export interface UsageRow {
  harness: string;
  model: string;
  tokens: UsageTokens;
  cost: number;
}

export interface UsageReport {
  today_cost: number;
  today_tokens: number;
  total_cost: number;
  total_tokens: number;
  rows: UsageRow[];
  api_priced: boolean;
}

/** M6: token + cost ledger from transcripts (on-demand; reads files). */
export async function getUsage(): Promise<UsageReport> {
  return invoke<UsageReport>("get_usage");
}

export interface ImessageStatus {
  enabled: boolean;
  approvals: boolean;
  fda_ok: boolean;
  detail: string;
}

export async function imessageStatus(): Promise<ImessageStatus> {
  return invoke<ImessageStatus>("imessage_status");
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function setSettings(
  settings: AppSettings,
): Promise<AppSettings> {
  return invoke<AppSettings>("set_settings", { settings });
}

export interface AccessRow {
  label: string;
  kind: string;
  detail: string;
  present: boolean;
}

export async function getAccess(): Promise<AccessRow[]> {
  return invoke<AccessRow[]>("get_access");
}

export interface HistoryRow {
  sid: string;
  title: string;
  harness: string;
  model: string;
  mtime: number;
  note: string;
  archived: boolean;
}

export async function listHistory(): Promise<HistoryRow[]> {
  return invoke<HistoryRow[]>("list_history");
}
