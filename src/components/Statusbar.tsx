import { doSetYolo, useStore } from "../store";

function healthLine(state: {
  health: {
    watcher: boolean;
    adapters: { harness: string; status: string }[];
    serve: boolean;
  };
}): string {
  const { watcher, adapters, serve } = state.health;
  const watch = watcher ? "watcher ok" : "watcher …";
  const short = (h: string) => {
    if (h === "claude code") return "claude";
    return h;
  };
  const ads =
    adapters.length === 0
      ? "adapters …"
      : adapters
          .map((a) => `${short(a.harness)} ${a.status}`)
          .join(" · ");
  const srv = serve ? "serve up" : "serve down";
  return `${watch} · ${ads} · ${srv}`;
}

export function Statusbar() {
  const { state, dispatch } = useStore();
  const sel = state.sessions[state.sel];
  const showSubsHint = (sel?.subs.length ?? 0) > 0;
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
        onClick={() => void doSetYolo(!state.yolo, dispatch)}
      >
        yolo <i>{state.yolo ? "on" : "off"}</i>
      </button>
      <span className="health" id="health" title="adapter + serve health">
        {healthLine(state)}
      </span>
      <span className="hints">
        <kbd>1–9</kbd> select · <kbd>j k</kbd> sessions
        {showSubsHint ? (
          <>
            {" "}
            · <kbd>h l</kbd> subagents
          </>
        ) : null}{" "}
        · <kbd>⇥</kbd> approve · <kbd>/</kbd> commands · <kbd>⌘K</kbd> menu
      </span>
    </div>
  );
}
