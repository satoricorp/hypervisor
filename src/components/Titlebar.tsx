import { useStore } from "../store";

export function Titlebar() {
  const { dispatch } = useStore();
  return (
    <div className="titlebar">
      <span className="mark">
        HYPERVISOR<small>v0.1 · variant B</small>
      </span>
      <button
        className="menukey"
        id="menukey"
        title="command palette"
        type="button"
        onClick={() => dispatch({ type: "OPEN_PALETTE" })}
      >
        ⌘K
      </button>
    </div>
  );
}
