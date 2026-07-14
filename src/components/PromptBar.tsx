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
      </>
    );
    // Ownership note lives here now (removed from the session list). Only a
    // tmux-owned session can be driven; claude/codex can be taken over (⏎),
    // cursor/opencode are follow-only (Hypervisor mirrors but can't drive them).
    if (s.ctl !== "tmux") {
      const adoptable = s.ctl === "observe" && !s.noAdopt;
      placeholder = adoptable
        ? "Hypervisor isn't driving this yet — press ⏎ to take it over"
        : "follow-only — Hypervisor mirrors this session but can't drive it";
    }
  }

  return (
    <div className="promptzone">
      <CommandMenu />
      <div className="promptbar">
        <div className="target" id="target">
          {targetInner}
        </div>
        <textarea
          id="prompt"
          ref={promptRef}
          rows={1}
          spellCheck={false}
          autoComplete="off"
          placeholder={placeholder}
          aria-label="prompt"
          value={state.prompt}
          onChange={(e) =>
            dispatch({ type: "SET_PROMPT", value: e.target.value })
          }
        />
        <span className="sendkey">⏎ send · ⇧⏎ newline</span>
      </div>
    </div>
  );
}
