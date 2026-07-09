import { useStore } from "../store";

export function NewAgentButton() {
  const { dispatch, promptRef } = useStore();
  return (
    <button
      className="newbtn"
      id="newbtn"
      type="button"
      onClick={() => {
        dispatch({ type: "START_NEW_AGENT" });
        requestAnimationFrame(() => promptRef.current?.focus());
      }}
    >
      + New Agent <kbd>⌘N</kbd>
    </button>
  );
}
