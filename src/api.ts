import { invoke } from "@tauri-apps/api/core";
import type { SessionWire } from "./wire";

export async function listSessions(): Promise<SessionWire[]> {
  return invoke<SessionWire[]>("list_sessions");
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

/** Wait up to 15s for a newly owned (tmux) sid that wasn't in `before`. */
export async function waitForOwnedSid(
  before: Set<string>,
  timeoutMs = 15_000,
): Promise<string | null> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const list = await listSessions();
    const found = list.find(
      (s) => s.control === "tmux" && s.sid && !before.has(s.sid),
    );
    if (found) return found.sid;
    await new Promise((r) => setTimeout(r, 500));
  }
  return null;
}
