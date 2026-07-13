import { Fragment, useEffect, useRef, useState } from "react";
import { renameSession } from "../api";
import { CTL_HINT, STATE_META, iconOf } from "../constants";
import { useStore } from "../store";
import type { Session } from "../types";
import { NewAgentButton } from "./NewAgentButton";

/** Model label without the vendor prefix: claude-opus-4-8 → opus-4-8. */
function cleanModel(m: string): string {
  if (!m || m === "—") return "";
  return m.replace(/^claude-/, "");
}

/** Harness label for the row: "claude code" → "claude". */
function appShort(app: string): string {
  return app === "claude code" ? "claude" : app;
}

/** Which section a session belongs to: Claude desktop-app chats group under
 *  "Claude"; otherwise by repo, with repo-less sessions under "Computer". */
function groupLabel(s: Session): string {
  if (s.entrypoint === "claude-desktop") return "Claude";
  return s.repo && s.repo !== "-" ? s.repo : "Computer";
}

export function Sidebar() {
  const { state, dispatch } = useStore();
  const selRef = useRef<HTMLDivElement | null>(null);
  const [editing, setEditing] = useState<{ sid: string; value: string } | null>(
    null,
  );

  useEffect(() => {
    selRef.current?.scrollIntoView({ block: "nearest" });
  }, [state.sel]);

  const overflow = Math.max(0, state.total - state.sessions.length);

  async function commitRename(sid: string, value: string) {
    setEditing(null);
    const title = value.trim();
    try {
      // Empty / "-" reverts to the harness-derived title (rename is a local
      // hypervisor override in titles.json — never written to the harness).
      await renameSession(sid, title.length ? title : "-");
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
    }
  }

  return (
    <aside id="side" aria-label="sessions">
      <NewAgentButton />
      {state.sessions.map((s, i) => {
        const m = STATE_META[s.state];
        const group = groupLabel(s);
        const showHeader =
          i === 0 || groupLabel(state.sessions[i - 1]) !== group;
        const ctl = CTL_HINT[s.ctl];
        const showCtl = s.ctl === "observe" || s.ctl === "watch";
        const isEditing = editing?.sid === s.sid;
        const model = cleanModel(s.model);
        const subs = (s.sidechains ?? 0) > 0 ? (s.sidechains ?? 0) : s.subs.length;
        return (
          <Fragment key={s.sid || `i${i}`}>
            {showHeader ? (
              <div
                className="grouphdr"
                title={
                  group === "Computer"
                    ? "sessions not tied to a repo — run jobs from anywhere"
                    : group === "Claude"
                      ? "chats launched from the Claude desktop app"
                      : group
                }
              >
                {group === "Computer"
                  ? "▚ Computer"
                  : group === "Claude"
                    ? "✳ Claude"
                    : `⎇ ${group}`}
              </div>
            ) : null}
            <div
              className={`srow ${i === state.sel ? "sel" : ""}`}
              data-i={i}
              ref={i === state.sel ? selRef : undefined}
              onClick={() => dispatch({ type: "SELECT", i })}
            >
              <span className={`status ${m.cls}`}>
                <i className="dot" />
              </span>
              <span className="num">
                {s.n != null ? s.n : i < 9 ? i + 1 : "·"}
              </span>
              {isEditing ? (
                <input
                  className="renameinput"
                  autoFocus
                  value={editing!.value}
                  onClick={(e) => e.stopPropagation()}
                  onChange={(e) =>
                    setEditing({ sid: editing!.sid, value: e.target.value })
                  }
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter")
                      void commitRename(editing!.sid, editing!.value);
                    else if (e.key === "Escape") setEditing(null);
                  }}
                  onBlur={() => void commitRename(editing!.sid, editing!.value)}
                />
              ) : (
                <span
                  className="t"
                  title="double-click to rename (hypervisor only)"
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    if (s.sid) setEditing({ sid: s.sid, value: s.title });
                  }}
                >
                  {s.title}
                </span>
              )}
              <span className="m">
                {s.app ? (
                  <span
                    className="apphint"
                    title={s.app}
                    dangerouslySetInnerHTML={{ __html: iconOf(s.app) }}
                  />
                ) : null}
                <span>
                  {appShort(s.app)}
                  {model ? ` · ${model}` : ""}
                </span>
                {s.approval ? <span className="pausechip">⏸</span> : null}
                {subs > 0 ? <span>↳ {subs}</span> : null}
                {showCtl && ctl ? (
                  <span className="obstag" title={ctl.tip}>
                    {s.ctl === "observe" ? "observe" : "watch"}
                  </span>
                ) : null}
                {s.loop ? <span className="loopchip">↻</span> : null}
              </span>
            </div>
          </Fragment>
        );
      })}
      {overflow > 0 ? (
        <div className="sidefoot" id="sidefoot">
          +{overflow} more · not monitored
        </div>
      ) : null}
    </aside>
  );
}
