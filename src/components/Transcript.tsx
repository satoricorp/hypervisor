import type { LogEntry, Session, Subagent } from "../types";

function logHtml(L: LogEntry[]) {
  return L.map((e, i) => {
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
  });
}

function nowBlock(s: Session) {
  if (s.state === "working") {
    const think = s.think ?? [];
    // Mockup maps think in order with caret on the last line; thinkIdx is
    // cycled but unused in nowBlock (kept for fidelity / future).
    return (
      <>
        <div className="dtool">
          ⚒ {s.tool}({s.toolArg})
        </div>
        {think.map((t, n) => (
          <div key={`${n}-${s.thinkIdx ?? 0}`} className="dthink">
            {t}
            {n === think.length - 1 ? <span className="caret">▌</span> : null}
          </div>
        ))}
      </>
    );
  }
  if (s.state === "done") {
    return (
      <>
        <div className="dresult">{s.result}</div>
        {(s.output || []).map((l, i) => (
          <div key={i} className="dagent">
            {l}
          </div>
        ))}
      </>
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
  return <div className="dfail">{s.fail}</div>;
}

export function Transcript({
  session,
  log,
}: {
  session: Session;
  log: LogEntry[];
}) {
  return (
    <div className="dlog" id="dlog">
      {logHtml(log)}
      <div id="nowblock">{nowBlock(session)}</div>
    </div>
  );
}

export function SubTranscript({ sub }: { sub: Subagent }) {
  const log =
    sub.log ??
    ([
      { k: "you" as const, t: sub.task },
      {
        k: "agent" as const,
        t: "picked up the handoff — running in an isolated worktree.",
      },
    ] as LogEntry[]);
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
      {logHtml(log)}
      {nowSub}
    </div>
  );
}
