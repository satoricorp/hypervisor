import { useEffect, useRef, useState } from "react";
import type { TranscriptItem } from "../api";
import type { Session } from "../types";

const THINK_CLAMP = 280;

/** For file-editing tools, pull the file name + added/removed line counts out
 *  of the tool input so an edit reads at a glance (#3). Null for other tools. */
function fileEditStat(
  name: string,
  input: string,
): { file: string; added: number; removed: number } | null {
  if (!/^(Edit|Write|MultiEdit|NotebookEdit)$/.test(name)) return null;
  let obj: Record<string, unknown>;
  try {
    obj = JSON.parse(input);
  } catch {
    return null;
  }
  const path = String(obj.file_path ?? obj.path ?? obj.notebook_path ?? "");
  const file = path.split("/").pop() || path;
  const lines = (s: unknown) => {
    const t = typeof s === "string" ? s : "";
    return t.length ? t.split("\n").length : 0;
  };
  if (name === "Write") return { file, added: lines(obj.content), removed: 0 };
  if (name === "MultiEdit" && Array.isArray(obj.edits)) {
    let added = 0;
    let removed = 0;
    for (const e of obj.edits as Record<string, unknown>[]) {
      added += lines(e.new_string);
      removed += lines(e.old_string);
    }
    return { file, added, removed };
  }
  return {
    file,
    added: lines(obj.new_string ?? obj.new_source),
    removed: lines(obj.old_string ?? obj.old_source),
  };
}

function ToolBlock({ item }: { item: Extract<TranscriptItem, { kind: "tool" }> }) {
  const [open, setOpen] = useState(false);
  const hint = item.summary ? `(${item.summary})` : "";
  const err = item.is_error;
  const edit = fileEditStat(item.name, item.input);
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
          {edit ? (
            <span className="dtooledit">
              {" "}
              <span className="dtoolfile">{edit.file}</span>
              {edit.added ? (
                <span className="diffadd"> +{edit.added}</span>
              ) : null}
              {edit.removed ? (
                <span className="diffdel"> −{edit.removed}</span>
              ) : null}
            </span>
          ) : (
            hint
          )}
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

/** Collapses a run of intermediate steps (thinking + tool calls) between a
 *  prompt and a response into one accordion, so the conversation reads clean. */
function StepsAccordion({ items }: { items: TranscriptItem[] }) {
  const [open, setOpen] = useState(false);
  const tools = items.filter((i) => i.kind === "tool").length;
  const thinks = items.filter((i) => i.kind === "thinking").length;
  const parts: string[] = [];
  if (tools) parts.push(`${tools} tool${tools > 1 ? "s" : ""}`);
  if (thinks) parts.push(`${thinks} thinking`);
  return (
    <div className="dsteps">
      <button
        type="button"
        className="dstepshead"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="dtoolchev">{open ? "▾" : "▸"}</span>
        <span className="dstepslabel">
          {items.length} step{items.length > 1 ? "s" : ""}
          {parts.length ? ` · ${parts.join(" · ")}` : ""}
        </span>
      </button>
      {open ? (
        <div className="dstepsbody">
          {items.map((it, i) =>
            it.kind === "thinking" ? (
              <ThinkingBlock key={i} text={it.text} />
            ) : it.kind === "tool" ? (
              <ToolBlock key={it.id || i} item={it} />
            ) : null,
          )}
        </div>
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

  // Group intermediate steps (thinking + tool calls) into collapsible
  // accordions so prompts (you) and responses (agent) stay prominent.
  type Row =
    | { k: "you"; text: string }
    | { k: "agent"; text: string }
    | { k: "steps"; items: TranscriptItem[] };
  const rows: Row[] = [];
  {
    let buf: TranscriptItem[] = [];
    const flush = () => {
      if (buf.length) {
        rows.push({ k: "steps", items: buf });
        buf = [];
      }
    };
    for (const it of items) {
      if (it.kind === "thinking" || it.kind === "tool") {
        buf.push(it);
      } else {
        flush();
        rows.push(
          it.kind === "user"
            ? { k: "you", text: it.text }
            : { k: "agent", text: it.text },
        );
      }
    }
    flush();
  }

  return (
    <div className="dlog" id="dlog" ref={ref}>
      {loading && items.length === 0 ? (
        <div className="dthink">loading transcript…</div>
      ) : null}
      {!loading && items.length === 0 ? (
        <div className="dthink">no transcript lines yet</div>
      ) : null}
      {rows.map((row, i) =>
        row.k === "you" ? (
          <div key={i} className="dyou">
            {row.text}
          </div>
        ) : row.k === "agent" ? (
          <div key={i} className="dagent">
            {row.text}
          </div>
        ) : (
          <StepsAccordion key={i} items={row.items} />
        ),
      )}
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
