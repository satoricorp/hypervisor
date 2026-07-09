import { useStore } from "../store";

export function CommandMenu() {
  const { state, dispatch } = useStore();
  const { menu } = state;
  if (!menu.open) {
    return (
      <div className="cmdmenu" id="cmdmenu" role="listbox">
        <div className="cmdcrumb" id="cmdcrumb">
          commands
        </div>
        <div className="cmditems" id="cmditems" />
        <div className="cmdhint">↑↓ navigate · ⏎ choose · esc back</div>
      </div>
    );
  }

  const who = menu.cmd === "new" ? "new session" : "subagents";
  const crumb =
    menu.step === "root"
      ? "commands"
      : menu.step === "target"
        ? menu.cmd === "new"
          ? "new session → pick harness"
          : `subagents → pick target · from "${state.sessions[state.sel].title}"`
        : `${who} → ${menu.target} → pick model`;

  return (
    <div className="cmdmenu open" id="cmdmenu" role="listbox">
      <div className="cmdcrumb" id="cmdcrumb">
        {crumb}
      </div>
      <div className="cmditems" id="cmditems">
        {menu.items.length ? (
          menu.items.map((it, i) => (
            <div
              key={it.id + i}
              className={`cmditem ${i === menu.active ? "active" : ""}`}
              data-i={i}
              onClick={() => {
                dispatch({ type: "MENU_ACTIVE", active: i });
                dispatch({ type: "CHOOSE_MENU" });
              }}
            >
              <span className="cl">{it.label}</span>
              <span className="cd">{it.desc || ""}</span>
            </div>
          ))
        ) : (
          <div className="cmditem">
            <span className="cd">no matching command</span>
          </div>
        )}
      </div>
      <div className="cmdhint">↑↓ navigate · ⏎ choose · esc back</div>
    </div>
  );
}
