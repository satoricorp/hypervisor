import { useStore } from "../store";

export function Statusbar() {
  const { state, dispatch } = useStore();
  return (
    <div className="statusbar">
      <div
        className="ticker"
        id="ticker"
        tabIndex={0}
        title="open usage"
        role="button"
        onClick={() => dispatch({ type: "SHOW_VIEW", view: "usage" })}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            dispatch({ type: "SHOW_VIEW", view: "usage" });
          }
        }}
      >
        <b>$4.51</b> · 2.41 MTOK
      </div>
      <button
        className={`yolobtn ${state.yolo ? "on" : ""}`}
        id="yolo"
        type="button"
        title="auto-approve every permission request"
        onClick={() => dispatch({ type: "SET_YOLO", on: !state.yolo })}
      >
        yolo <i>{state.yolo ? "on" : "off"}</i>
      </button>
      <span className="hints">
        <kbd>1–9</kbd> select · <kbd>j k</kbd> sessions · <kbd>h l</kbd>{" "}
        subagents · <kbd>⇥</kbd> approve · <kbd>/</kbd> commands ·{" "}
        <kbd>⌘K</kbd> menu
      </span>
    </div>
  );
}
