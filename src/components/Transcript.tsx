import { useEffect, useRef, useState } from "react";
import type { TranscriptItem } from "../api";
import type { Session } from "../types";

const THINK_CLAMP = 280;

function ToolBlock({ item }: { item: Extract<TranscriptItem, { kind: "tool" }> }) {
  const [open, setOpen] = useState(false);
  const hint = item.summary ? `(${item.summary})` : "";
  const err = item.is_error;
  return (
    <div className={`dtoolblock${err ? " dtool-err" : ""}`}>
      <button
        type="button"
        className="dtoolhead"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="dtoolchev">{open ? "▾" : "▸"}</span>
        <span className="dtool">
          ⚒ {item.name}
          {hint}
        </span>
        {err ? <span className="dtoolerrchip">error</span> : null}
      </button>
      {open ? (
        <div className="dtoolexpand">
          <div className="dtoollabel">input</div>
          <pre className="dtoolpre">{item.input || "—"}</pre>
          <div className="dtoollabel">result</div>
          <pre className={`dtoolpre${err ? " dtoolpre-err" : ""}`}>
            {item.result ?? "(pending)"}
          </pre>
        </div>
      ) : null}
    </div>
  );
}

function ThinkingBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = text.length > THINK_CLAMP;
  const shown = !long || open ? text : `${text.slice(0, THINK_CLAMP)}…`;
  return (
    <div
      className={`dthink${long ? " dthink-clamp" : ""}`}
      onClick={() => long && setOpen((v) => !v)}
      role={long ? "button" : undefined}
      tabIndex={long ? 0 : undefined}
      onKeyDown={(e) => {
        if (long && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          setOpen((v) => !v);
        }
      }}
    >
      {shown}
      {long ? (
        <span className="dthinkmore">{open ? " · less" : " · more"}</span>
      ) : null}
    </div>
  );
}

function nowBlock(s: Session) {
  if (s.state === "working") {
    if (s.tool) {
      return (
        <div className="dtool">
          ⚒ {s.tool}
          {s.toolArg ? `(${s.toolArg})` : ""}
          <span className="caret">▌</span>
        </div>
      );
    }
    return (
      <div className="dthink">
        working<span className="caret">▌</span>
      </div>
    );
  }
  if (s.state === "input") {
    if (s.approval) {
      return (
        <>
          <div className="dask">⏸ wants to run — {s.approval}</div>
          <div className="dthink">
            ⇥ approve · or respond below to deny with guidance
          </div>
        </>
      );
    }
    return <div className="dask">{s.ask}</div>;
  }
  if (s.state === "error") {
    return <div className="dfail">{s.fail}</div>;
  }
  return null;
}

export function TranscriptView({
  session,
  items,
  loading,
}: {
  session: Session;
  items: TranscriptItem[];
  loading: boolean;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const pinned = useRef(true);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
      pinned.current = gap < 48;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el || !pinned.current) return;
    el.scrollTop = el.scrollHeight;
  }, [items, session.state, session.tool, session.approval, loading]);

  return (
    <div className="dlog" id="dlog" ref={ref}>
      {loading && items.length === 0 ? (
        <div className="dthink">loading transcript…</div>
      ) : null}
      {!loading && items.length === 0 ? (
        <div className="dthink">no transcript lines yet</div>
      ) : null}
      {items.map((it, i) => {
        if (it.kind === "user") {
          return (
            <div key={i} className="dyou">
              {it.text}
            </div>
          );
        }
        if (it.kind === "assistant") {
          return (
            <div key={i} className="dagent">
              {it.text}
            </div>
          );
        }
        if (it.kind === "thinking") {
          return <ThinkingBlock key={i} text={it.text} />;
        }
        return <ToolBlock key={it.id || i} item={it} />;
      })}
      <div id="nowblock">{nowBlock(session)}</div>
    </div>
  );
}

/** Legacy subagent mock transcript — unchanged. */
export function SubTranscript({
  sub,
}: {
  sub: {
    task: string;
    state: string;
    log?: { k: string; t: string }[];
  };
}) {
  const log =
    sub.log ??
    ([
      { k: "you" as const, t: sub.task },
      {
        k: "agent" as const,
        t: "picked up the handoff — running in an isolated worktree.",
      },
    ] as { k: string; t: string }[]);
  const nowSub =
    sub.state === "working" ? (
      <>
        <div className="dtool">⚒ Bash(npm test · watching)</div>
        <div className="dthink">
          iterating until green<span className="caret">▌</span>
        </div>
      </>
    ) : (
      <div className="dresult">
        ✓ handoff complete — results reported back to the orchestrator
      </div>
    );
  return (
    <div className="dlog">
      {log.map((e, i) => {
        if (e.k === "you")
          return (
            <div key={i} className="dyou">
              {e.t}
            </div>
          );
        if (e.k === "tool")
          return (
            <div key={i} className="dtool">
              ⚒ {e.t}
            </div>
          );
        return (
          <div key={i} className="dagent">
            {e.t}
          </div>
        );
      })}
      {nowSub}
    </div>
  );
}
