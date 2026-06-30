#!/usr/bin/env python3
"""hvwatch — Hypervisor spike: one status line per AI agent session, across harnesses.

Read-only adapters over each harness's on-disk session state:

  claude code   ~/.claude/projects/<proj>/<session>.jsonl        transcripts (JSONL)
  codex         ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl     rollouts (JSONL)
  cursor        ~/Library/Application Support/Cursor/User/       state.vscdb (sqlite, best-effort)

No dependencies. Never writes to any harness file.

Usage:
  hvwatch.py                live dashboard (refresh every 2s)
  hvwatch.py --once         single snapshot
  hvwatch.py --json         emit sessions as JSON lines (future history.db feed)
  hvwatch.py --max-age 48   only sessions touched in the last N hours
  hvwatch.py --limit 8      max sessions per harness
"""

import argparse
import datetime as dt
import glob
import json
import os
import shutil
import sqlite3
import sys
import time

HOME = os.path.expanduser("~")
ACTIVE_S = 15          # writes within this window => working
STALL_S = 90           # you spoke last + no writes for this long => stalled
TAIL_BYTES = 512 * 1024

# ---------------------------------------------------------------- helpers

def clip(s, n):
    s = " ".join((s or "").split())
    return s if len(s) <= n else s[: n - 1] + "…"

def age_str(secs):
    if secs < 60: return f"{int(secs)}s"
    if secs < 3600: return f"{int(secs // 60)}m"
    if secs < 86400: return f"{int(secs // 3600)}h"
    return f"{int(secs // 86400)}d"

def read_lines(path):
    """All lines for small files; head+tail for big ones (transcripts grow large)."""
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as f:
            if size <= 2 * TAIL_BYTES:
                data = f.read()
            else:
                head = f.read(TAIL_BYTES)
                f.seek(size - TAIL_BYTES)
                tail = f.read()
                # drop the partial line at the start of the tail chunk
                data = head + b"\n" + tail[tail.index(b"\n") + 1:]
        return data.decode("utf-8", "replace").splitlines()
    except OSError:
        return []

def jline(line):
    try:
        v = json.loads(line)
        return v if isinstance(v, dict) else None
    except (ValueError, RecursionError):
        return None

def is_noise(text):
    """Skip harness plumbing that lives in user slots (XML wrappers, AGENTS.md blobs)."""
    t = (text or "").lstrip()
    return (not t or t.startswith("<") or t.startswith("Caveat:")
            or t.startswith("# AGENTS.md") or "<INSTRUCTIONS>" in t[:400])

def session_state(mtime, last_role):
    idle = time.time() - mtime
    if idle <= ACTIVE_S:
        return "working"
    if last_role == "user" and idle > STALL_S:
        return "stalled"          # you asked; agent went quiet
    return "done"                 # last word was the agent's — waiting on you

# ---------------------------------------------------------------- adapters

def scan_claude(max_age_h, limit):
    out = []
    for path in glob.glob(f"{HOME}/.claude/projects/*/*.jsonl"):
        try:
            mtime = os.path.getmtime(path)
        except OSError:
            continue
        if time.time() - mtime > max_age_h * 3600:
            continue
        s = {
            "harness": "claude code", "sid": os.path.basename(path)[:-6],
            "title": "", "model": "", "cwd": "", "branch": "",
            "last_user": "", "activity": "", "last_assistant": "",
            "mtime": mtime, "src": path, "last_role": "", "sidechains": 0,
        }
        for line in read_lines(path):
            e = jline(line)
            if not e:
                continue
            if e.get("isSidechain"):
                # each subagent sidechain opens with a parentless user entry
                if e.get("type") == "user" and not e.get("parentUuid"):
                    s["sidechains"] += 1
                continue
            s["cwd"] = e.get("cwd") or s["cwd"]
            s["branch"] = e.get("gitBranch") or s["branch"]
            typ, msg = e.get("type"), e.get("message") or {}
            content = msg.get("content")
            if typ == "user" and not e.get("isMeta"):
                texts = []
                if isinstance(content, str):
                    texts = [content]
                elif isinstance(content, list):
                    texts = [c.get("text", "") for c in content
                             if isinstance(c, dict) and c.get("type") == "text"]
                for t in texts:
                    if not is_noise(t):
                        s["last_user"] = t
                        s["last_role"] = "user"
                        if not s["title"]:
                            s["title"] = t
            elif typ == "assistant":
                s["model"] = msg.get("model") or s["model"]
                for c in content if isinstance(content, list) else []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "tool_use":
                        arg = c.get("input") or {}
                        hint = arg.get("file_path") or arg.get("path") or \
                               clip(str(arg.get("command", "")), 40) or \
                               clip(str(arg.get("pattern", "")), 40)
                        s["activity"] = f"⚒ {c.get('name')}({clip(str(hint), 46)})"
                        s["last_role"] = "assistant"
                    elif c.get("type") == "text" and c.get("text"):
                        s["last_assistant"] = c["text"]
                        s["last_role"] = "assistant"
        if s["title"]:
            out.append(s)
    out.sort(key=lambda x: -x["mtime"])
    return out[:limit]

def scan_codex(max_age_h, limit):
    out = []
    for path in glob.glob(f"{HOME}/.codex/sessions/*/*/*/rollout-*.jsonl"):
        try:
            mtime = os.path.getmtime(path)
        except OSError:
            continue
        if time.time() - mtime > max_age_h * 3600:
            continue
        s = {
            "harness": "codex", "sid": os.path.basename(path)[:-6][-8:],
            "title": "", "model": "", "cwd": "", "branch": "",
            "last_user": "", "activity": "", "last_assistant": "",
            "mtime": mtime, "src": path, "last_role": "", "sidechains": 0,
        }
        for line in read_lines(path):
            e = jline(line)
            if not e:
                continue
            typ, p = e.get("type"), e.get("payload") or {}
            if typ == "session_meta":
                s["cwd"] = p.get("cwd") or s["cwd"]
                git = p.get("git") or {}
                s["branch"] = git.get("branch") or s["branch"]
            elif typ == "turn_context":
                s["model"] = p.get("model") or s["model"]
                s["cwd"] = p.get("cwd") or s["cwd"]
            elif typ == "response_item":
                pt = p.get("type")
                if pt == "message":
                    texts = [c.get("text", "") for c in p.get("content") or []
                             if isinstance(c, dict) and c.get("type") in ("input_text", "output_text")]
                    text = next((t for t in texts if not is_noise(t)), "")
                    if not text:
                        continue
                    if p.get("role") == "user":
                        s["last_user"] = text
                        s["last_role"] = "user"
                        if not s["title"]:
                            s["title"] = text
                    else:
                        s["last_assistant"] = text
                        s["last_role"] = "assistant"
                elif pt in ("function_call", "local_shell_call", "custom_tool_call"):
                    name = p.get("name") or pt
                    arg = p.get("arguments") or str(p.get("action") or "")
                    s["activity"] = f"⚒ {name}({clip(str(arg), 46)})"
                    s["last_role"] = "assistant"
                elif pt == "reasoning":
                    for sm in p.get("summary") or []:
                        if isinstance(sm, dict) and sm.get("text"):
                            s["last_assistant"] = sm["text"]
            elif typ == "event_msg" and (p.get("type") == "agent_message"):
                if p.get("message"):
                    s["last_assistant"] = p["message"]
                    s["last_role"] = "assistant"
        if s["title"]:
            out.append(s)
    out.sort(key=lambda x: -x["mtime"])
    return out[:limit]

def _sqlite_ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro&immutable=1", uri=True, timeout=1)

def scan_cursor(max_age_h, limit):
    """Cursor (2026 schema): composerHeaders table in globalStorage/state.vscdb —
    one row per composer with a JSON `value` (name, unifiedMode, hasUnreadMessages)
    and a native isSubagent flag. Undocumented and version-dependent; best-effort."""
    base = f"{HOME}/Library/Application Support/Cursor/User"
    db = os.path.join(base, "globalStorage", "state.vscdb")
    if not os.path.exists(db):
        return []
    # workspaceId -> folder path, from workspaceStorage/*/workspace.json
    folders = {}
    for wj in glob.glob(f"{base}/workspaceStorage/*/workspace.json"):
        try:
            with open(wj) as f:
                folders[os.path.basename(os.path.dirname(wj))] = \
                    (json.load(f).get("folder") or "").replace("file://", "")
        except (OSError, ValueError):
            pass
    now = time.time()
    try:
        con = _sqlite_ro(db)
        rows = con.execute(
            "SELECT composerId, workspaceId, lastUpdatedAt, isSubagent, value "
            "FROM composerHeaders WHERE isArchived=0 "
            "ORDER BY lastUpdatedAt DESC LIMIT ?", (limit * 4,)).fetchall()
        con.close()
    except sqlite3.Error:
        return []
    out, subcount = [], {}
    for cid, wsid, upd, is_sub, value in rows:
        ts = (upd or 0) / 1000.0
        if not ts or now - ts > max_age_h * 3600:
            continue
        if is_sub:
            subcount[wsid] = subcount.get(wsid, 0) + 1
            continue
        try:
            v = json.loads(value) if value else {}
        except (ValueError, TypeError):
            v = {}
        out.append({
            "harness": "cursor", "sid": (cid or "")[:8],
            "title": v.get("name") or "untitled composer",
            "model": v.get("modelName") or v.get("unifiedMode") or "",
            "cwd": folders.get(wsid, ""), "branch": "",
            "last_user": "", "activity": "", "wsid": wsid,
            "last_assistant": "unread response" if v.get("hasUnreadMessages") else "",
            "mtime": ts, "src": db, "last_role": "assistant", "sidechains": 0,
        })
    for s in out:  # attach recent subagent count to that workspace's newest session
        s["sidechains"] = subcount.pop(s.pop("wsid", None), 0)
    out.sort(key=lambda x: -x["mtime"])
    return out[:limit]

# ---------------------------------------------------------------- render

C = {"working": "\x1b[33m", "done": "\x1b[32m", "stalled": "\x1b[31m"}
DIM, BOLD, RST = "\x1b[2m", "\x1b[1m", "\x1b[0m"

def finalize(sessions):
    for s in sessions:
        s["state"] = session_state(s["mtime"], s["last_role"])
        s["age"] = age_str(time.time() - s["mtime"])
        s["repo"] = os.path.basename(s["cwd"] or "") or "-"
    return sessions

def render(sessions, width):
    lines = []
    n_working = sum(1 for s in sessions if s["state"] == "working")
    now = dt.datetime.now().strftime("%H:%M:%S")
    lines.append(f"{BOLD}HYPERVISOR{RST}{DIM} spike · {now} · "
                 f"{len(sessions)} sessions · {n_working} working{RST}")
    lines.append("")
    for i, s in enumerate(sessions):
        dot = f"{C[s['state']]}●{RST}"
        repo = s["repo"] + (f" ⎇ {s['branch']}" if s["branch"] else "")
        model = clip(s["model"].replace("claude-", ""), 18) or "-"
        head = (f" {dot} {BOLD}{i+1:>2}{RST} {clip(s['title'], width - 40)}")
        meta = f"{DIM}{s['harness']} · {model} · {repo} · {s['age']}{RST}"
        lines.append(head)
        lines.append(f"      {meta}")
        detail = s["activity"] if s["state"] == "working" else clip(s["last_assistant"], width - 12)
        if s["sidechains"]:
            detail = f"↳ {s['sidechains']} subagent(s) · {detail}"
        if detail:
            lines.append(f"      {DIM}{clip(detail, width - 8)}{RST}")
        lines.append("")
    lines.append(f"{DIM}states: \x1b[33m●{RST}{DIM} working  \x1b[32m●{RST}{DIM} done/idle  "
                 f"\x1b[31m●{RST}{DIM} stalled (you spoke last) · read-only · ctrl-c quits{RST}")
    return "\n".join(lines)

def scan_all(args):
    sessions = []
    for fn in (scan_claude, scan_codex, scan_cursor):
        try:
            sessions += fn(args.max_age, args.limit)
        except Exception as e:  # an adapter must never take down the board
            print(f"{DIM}[{fn.__name__}] {e}{RST}", file=sys.stderr)
    sessions.sort(key=lambda s: -s["mtime"])
    return finalize(sessions)

def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--once", action="store_true", help="single snapshot, no loop")
    ap.add_argument("--json", action="store_true", help="emit JSON lines instead of a dashboard")
    ap.add_argument("--interval", type=float, default=2.0)
    ap.add_argument("--max-age", type=float, default=48, help="hours")
    ap.add_argument("--limit", type=int, default=8, help="sessions per harness")
    args = ap.parse_args()

    if args.json:
        for s in scan_all(args):
            print(json.dumps({k: v for k, v in s.items() if k != "last_role"},
                             ensure_ascii=False))
        return

    while True:
        width = shutil.get_terminal_size((120, 40)).columns
        board = render(scan_all(args), width)
        if args.once:
            print(board)
            return
        sys.stdout.write("\x1b[2J\x1b[H" + board + "\n")
        sys.stdout.flush()
        time.sleep(args.interval)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
