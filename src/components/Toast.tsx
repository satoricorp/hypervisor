import { useStore } from "../store";

export function Toast() {
  const { state } = useStore();
  return (
    <div
      id="toast"
      role="status"
      className={state.toasts.show ? "show" : ""}
      dangerouslySetInnerHTML={{ __html: state.toasts.html }}
    />
  );
}
