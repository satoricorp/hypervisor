import { toggleTv } from "../api";
import { useStore } from "../store";

export function Palette() {
  const { state, dispatch, palInputRef, promptRef } = useStore();
  const { palette } = state;

  return (
    <div
      id="palette"
      className={palette.open ? "open" : ""}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) dispatch({ type: "CLOSE_PALETTE" });
      }}
    >
      <div className="palpanel">
        <input
          id="palinput"
          ref={palInputRef}
          type="text"
          placeholder="where to? — views, commands"
          spellCheck={false}
          autoComplete="off"
          aria-label="command palette"
          value={palette.filter}
          onChange={(e) =>
            dispatch({ type: "SET_PAL_FILTER", value: e.target.value })
          }
        />
        <div className="cmditems" id="palitems" role="listbox">
          {palette.items.length ? (
            palette.items.map((it, i) => (
              <div
                key={it.id}
                className={`cmditem ${i === palette.active ? "active" : ""}`}
                data-i={i}
                onClick={() => {
                  dispatch({ type: "PAL_ACTIVE", active: i });
                  dispatch({ type: "CHOOSE_PAL" });
                  if (it.id === "subagents") {
                    requestAnimationFrame(() => promptRef.current?.focus());
                  }
                  if (it.id === "tv") {
                    toggleTv().catch((err) =>
                      dispatch({ type: "TOAST", label: "tv", detail: String(err) }),
                    );
                  }
                }}
              >
                <span className="cl">{it.label}</span>
                <span className="cd">{it.desc}</span>
              </div>
            ))
          ) : (
            <div className="cmditem">
              <span className="cd">nothing matches</span>
            </div>
          )}
        </div>
        <div className="cmdhint">↑↓ navigate · ⏎ go · esc close</div>
      </div>
    </div>
  );
}
