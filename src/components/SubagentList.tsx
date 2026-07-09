import { STATE_META, iconOf } from "../constants";
import { useStore } from "../store";

export function SubagentList() {
  const { state, dispatch } = useStore();
  const s = state.sessions[state.sel];
  if (!s.subs.length) return null;
  return (
    <div className="subswrap">
      {s.subs.map((x, j) => (
        <div
          key={j}
          className={`subrow ${state.subSel === j ? "ssel" : ""}`}
          data-j={j}
          onClick={() => dispatch({ type: "SET_SUB_SEL", j })}
        >
          <span className={`status ${STATE_META[x.state].cls}`}>
            <i className="dot" />
          </span>
          <span className="num">
            {state.sel + 1}·{j + 1}
          </span>
          <span className="subtask">{x.task || ""}</span>
          <span className="modelchip">{x.model}</span>
          <span
            className="hicon"
            title={x.target}
            dangerouslySetInnerHTML={{ __html: iconOf(x.target) }}
          />
        </div>
      ))}
    </div>
  );
}
