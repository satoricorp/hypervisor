import { useEffect, useRef } from "react";
import { STATE_META } from "../constants";
import { useStore } from "../store";
import { NewAgentButton } from "./NewAgentButton";

export function Sidebar() {
  const { state, dispatch } = useStore();
  const selRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    selRef.current?.scrollIntoView({ block: "nearest" });
  }, [state.sel]);

  return (
    <aside id="side" aria-label="sessions">
      <NewAgentButton />
      {state.sessions.map((s, i) => {
        const m = STATE_META[s.state];
        return (
          <div
            key={i}
            className={`srow ${i === state.sel ? "sel" : ""}`}
            data-i={i}
            ref={i === state.sel ? selRef : undefined}
            onClick={() => dispatch({ type: "SELECT", i })}
          >
            <span className={`status ${m.cls}`}>
              <i className="dot" />
            </span>
            <span className="num">{i < 9 ? i + 1 : "·"}</span>
            <span className="t">{s.title}</span>
            <span className="m">
              {s.approval ? "⏸ approval · " : ""}
              {s.subs.length ? `↳ ${s.subs.length} · ` : ""}
              {s.model}
              {s.repo ? ` · ${s.repo}` : ""}
              {s.ctl === "observe" ? <span className="obstag">obs</span> : null}
              {s.loop ? <span className="loopchip">↻</span> : null}
            </span>
          </div>
        );
      })}
    </aside>
  );
}
