import { STATE_META } from "../constants";
import { useStore } from "../store";
import { CommandMenu } from "./CommandMenu";

export function PromptBar() {
  const { state, dispatch, promptRef } = useStore();
  const s = state.sessions[state.sel];
  const inSub = s && state.subSel >= 0 && s.subs[state.subSel];

  let placeholder = "prompt selected session — or / for commands";
  let targetInner;
  if (!s) {
    targetInner = (
      <>
        <i className="tdot" style={{ background: "var(--dim)" }} />
        <span>—</span>
      </>
    );
    placeholder = "no sessions — + New Agent or /new";
  } else if (inSub) {
    const m = STATE_META[s.subs[state.subSel].state];
    targetInner = (
      <>
        <i className="tdot" style={{ background: m.color }} />
        <span>
          {state.sel + 1}·{state.subSel + 1}
        </span>
      </>
    );
    placeholder = "prompt selected subagent — or / for commands";
  } else {
    const m = STATE_META[s.state];
    targetInner = (
      <>
        <i className="tdot" style={{ background: m.color }} />
        <span>{state.sel + 1}</span>
        {s.ctl === "observe" ? <span className="obstag">obs</span> : null}
      </>
    );
    if (s.ctl === "observe") {
      placeholder = s.noAdopt
        ? "observe-only — no control path for this source yet"
        : `observe-only — ⏎ adopts into hypervisor tmux (claude --resume ${s.sid || "…"})`;
    }
  }

  return (
    <div className="promptzone">
      <CommandMenu />
      <div className="promptbar">
        <div className="target" id="target">
          {targetInner}
        </div>
        <input
          id="prompt"
          ref={promptRef}
          type="text"
          spellCheck={false}
          autoComplete="off"
          placeholder={placeholder}
          aria-label="prompt"
          value={state.prompt}
          onChange={(e) =>
            dispatch({ type: "SET_PROMPT", value: e.target.value })
          }
        />
        <span className="sendkey">⏎ send</span>
      </div>
    </div>
  );
}
