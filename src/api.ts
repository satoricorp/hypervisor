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
): Promise<string> {
  return invoke<string>("spawn_session", {
    harness,
    model,
    cwd: cwd ?? null,
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
