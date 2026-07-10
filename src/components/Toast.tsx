import { useStore } from "../store";

export function Toast() {
  const { state } = useStore();
  const { label, detail, show } = state.toasts;
  return (
    <div id="toast" role="status" className={show ? "show" : ""}>
      {label ? <b>{label}</b> : null}
      {detail ? (
        <>
          {label ? " · " : null}
          {detail}
        </>
      ) : null}
    </div>
  );
}
