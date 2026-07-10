import {
  createContext,
  useContext,
  useReducer,
  useEffect,
  useRef,
  type Dispatch,
  type ReactNode,
  type RefObject,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { MODELS, PAL, ROOT_CMDS, TARGETS } from "./menuData";
import { ensureLog } from "./constants";
import { adoptSession, listSessions, sendPrompt, spawnSession, waitForOwnedSid } from "./api";
import { wireToSession, type SessionWire } from "./wire";
import type {
  AppState,
  MenuItem,
  Session,
  ViewName,
} from "./types";

function deepCloneSessions(sessions: Session[]): Session[] {
  return structuredClone(sessions);
}

function menuItemsFor(state: AppState): MenuItem[] {
  const { menu, prompt } = state;
  if (menu.step === "root") {
    const f = prompt.slice(1).toLowerCase();
    return ROOT_CMDS.filter((c) => c.label.slice(1).startsWith(f));
  }
  if (menu.step === "target") return TARGETS;
  const models = MODELS[menu.target ?? ""] ?? [];
  return models.map((m) => ({ id: m, label: m, desc: "" }));
}

function palItemsFor(filter: string): MenuItem[] {
  const f = filter.trim().toLowerCase();
  return PAL.filter(
    (x) =>
      !f ||
      x.label.toLowerCase().includes(f) ||
      x.desc.toLowerCase().includes(f),
  );
}

function withMenuItems(state: AppState): AppState {
  const items = menuItemsFor(state);
  const active = Math.min(state.menu.active, Math.max(0, items.length - 1));
  return { ...state, menu: { ...state.menu, items, active } };
}

function withPalItems(state: AppState): AppState {
  const items = palItemsFor(state.palette.filter);
  const active = Math.min(
    state.palette.active,
    Math.max(0, items.length - 1),
  );
  return { ...state, palette: { ...state.palette, items, active } };
}

export const initialState: AppState = withPalItems(
  withMenuItems({
    sessions: [],
    sel: 0,
    subSel: -1,
    view: "session",
    menu: {
      open: false,
      step: "root",
      cmd: "subagents",
      target: null,
      active: 0,
      items: [],
    },
    palette: { open: false, active: 0, filter: "", items: [] },
    yolo: false,
    toasts: { html: "", show: false },
    prompt: "",
    historyFilter: "",
  }),
);

export type Action =
  | { type: "SELECT"; i: number }
  | { type: "MOVE_SUB"; dir: number }
  | { type: "SET_SUB_SEL"; j: number }
  | { type: "SHOW_VIEW"; view: ViewName }
  | { type: "SET_PROMPT"; value: string }
  | { type: "SET_HISTORY_FILTER"; value: string }
  | { type: "TOAST"; html: string }
  | { type: "HIDE_TOAST" }
  | { type: "OPEN_MENU" }
  | { type: "CLOSE_MENU" }
  | { type: "DRAW_MENU" }
  | { type: "MENU_ACTIVE"; active: number }
  | { type: "MENU_STEP_BACK" }
  | { type: "OPEN_PALETTE" }
  | { type: "CLOSE_PALETTE" }
  | { type: "SET_PAL_FILTER"; value: string }
  | { type: "PAL_ACTIVE"; active: number }
  | { type: "SET_YOLO"; on: boolean }
  | { type: "SET_SESSIONS"; sessions: Session[] }
  | { type: "APPROVE_SEL" }
  | { type: "CHOOSE_MENU" }
  | { type: "CHOOSE_PAL" }
  | { type: "START_NEW_AGENT" }
  | { type: "OPEN_SUBAGENTS_FROM_PAL" }
  | { type: "CLEAR_PROMPT" }
  | { type: "OPTIMISTIC_SENT"; i: number; text: string }
  | {
      type: "REQUEST_SPAWN";
      cmd: string;
      target: string;
      model: string;
    };

function toast(state: AppState, html: string): AppState {
  return { ...state, toasts: { html, show: true } };
}

function select(state: AppState, i: number): AppState {
  const len = state.sessions.length;
  if (len === 0) return { ...state, sel: 0, subSel: -1 };
  const sel = ((i % len) + len) % len;
  return {
    ...state,
    sel,
    subSel: -1,
    view: state.view !== "session" ? "session" : state.view,
  };
}

function moveSub(state: AppState, dir: number): AppState {
  const s = state.sessions[state.sel];
  if (!s || !s.subs.length) return state;
  let subSel = state.subSel;
  if (dir > 0) subSel = Math.min(subSel + 1, s.subs.length - 1);
  else subSel = subSel <= 0 ? -1 : subSel - 1;
  return {
    ...state,
    subSel,
    view: state.view !== "session" ? "session" : state.view,
  };
}

function mutateSessions(
  state: AppState,
  fn: (sessions: Session[]) => void,
): AppState {
  const sessions = deepCloneSessions(state.sessions);
  fn(sessions);
  return { ...state, sessions };
}

function esc(s: string): string {
  return String(s).replace(
    /[&<>"]/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string,
  );
}

function harnessLabel(target: string): string {
  return target === "claude" ? "claude code" : target;
}

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "SELECT":
      return select(state, action.i);
    case "MOVE_SUB":
      return moveSub(state, action.dir);
    case "SET_SUB_SEL":
      return { ...state, subSel: action.j };
    case "SHOW_VIEW":
      return { ...state, view: action.view };
    case "SET_PROMPT": {
      let next = { ...state, prompt: action.value };
      if (action.value.startsWith("/")) {
        if (!next.menu.open) {
          next = withMenuItems({
            ...next,
            menu: {
              ...next.menu,
              open: true,
              step: "root",
              cmd: "subagents",
              target: null,
              active: 0,
            },
          });
        } else if (next.menu.step === "root") {
          next = withMenuItems(next);
        }
      } else if (next.menu.open) {
        next = { ...next, menu: { ...next.menu, open: false } };
      }
      return next;
    }
    case "SET_HISTORY_FILTER":
      return { ...state, historyFilter: action.value };
    case "TOAST":
      return toast(state, action.html);
    case "HIDE_TOAST":
      return { ...state, toasts: { ...state.toasts, show: false } };
    case "OPEN_MENU":
      return withMenuItems({
        ...state,
        menu: {
          ...state.menu,
          open: true,
          step: "root",
          cmd: "subagents",
          target: null,
          active: 0,
        },
      });
    case "CLOSE_MENU":
      return { ...state, menu: { ...state.menu, open: false } };
    case "DRAW_MENU":
      return withMenuItems(state);
    case "MENU_ACTIVE":
      return { ...state, menu: { ...state.menu, active: action.active } };
    case "MENU_STEP_BACK": {
      if (state.menu.step === "model") {
        return withMenuItems({
          ...state,
          menu: { ...state.menu, step: "target", active: 0 },
        });
      }
      if (state.menu.step === "target") {
        return withMenuItems({
          ...state,
          menu: { ...state.menu, step: "root", active: 0 },
        });
      }
      return { ...state, menu: { ...state.menu, open: false } };
    }
    case "OPEN_PALETTE":
      return withPalItems({
        ...state,
        palette: { ...state.palette, open: true, filter: "", active: 0 },
      });
    case "CLOSE_PALETTE":
      return {
        ...state,
        palette: { ...state.palette, open: false },
      };
    case "SET_PAL_FILTER":
      return withPalItems({
        ...state,
        palette: { ...state.palette, filter: action.value, active: 0 },
      });
    case "PAL_ACTIVE":
      return {
        ...state,
        palette: { ...state.palette, active: action.active },
      };
    case "SET_YOLO":
      return toast(
        { ...state, yolo: action.on },
        action.on
          ? "yolo on — auto-approving everything. godspeed."
          : "yolo off — approvals wait for ⇥ again",
      );
    case "SET_SESSIONS": {
      const sessions = action.sessions;
      let sel = state.sel;
      if (sessions.length === 0) sel = 0;
      else if (sel >= sessions.length) sel = sessions.length - 1;
      const prevSid = state.sessions[state.sel]?.sid;
      if (prevSid) {
        const idx = sessions.findIndex((s) => s.sid === prevSid);
        if (idx >= 0) sel = idx;
      }
      return { ...state, sessions, sel };
    }
    case "CLEAR_PROMPT":
      return { ...state, prompt: "" };
    case "OPTIMISTIC_SENT":
      return mutateSessions(state, (sessions) => {
        const s = sessions[action.i];
        if (!s) return;
        s.sent = action.text;
        s.state = "working";
        ensureLog(s);
        s.log!.push({ k: "you", t: action.text });
      });
    case "APPROVE_SEL":
      return toast(state, "approvals land in M3");
    case "REQUEST_SPAWN":
      // Handled in StoreProvider effect; reducer just closes menu.
      return {
        ...state,
        menu: { ...state.menu, open: false },
        prompt: "",
      };
    case "CHOOSE_MENU": {
      const it = state.menu.items[state.menu.active];
      if (!it) return state;
      if (state.menu.step === "root") {
        if (it.id === "subagents" || it.id === "new") {
          return withMenuItems({
            ...state,
            menu: {
              ...state.menu,
              cmd: it.id,
              step: "target",
              active: 0,
            },
          });
        }
        const next: AppState = {
          ...state,
          menu: { ...state.menu, open: false },
          prompt: "",
        };
        if (
          it.id === "plan" ||
          it.id === "review" ||
          it.id === "loop" ||
          it.id === "worktree" ||
          it.id === "broadcast"
        ) {
          return toast(next, "lands in M3/M4");
        }
        return toast(
          next,
          `<b>/${esc(it.id)}</b> — concept only, not wired yet`,
        );
      }
      if (state.menu.step === "target") {
        return withMenuItems({
          ...state,
          menu: {
            ...state.menu,
            target: it.id,
            step: "model",
            active: 0,
          },
        });
      }
      // model step — close menu; spawn kicked off by keyboard/effect via REQUEST_SPAWN
      return {
        ...state,
        menu: { ...state.menu, open: false },
        prompt: "",
      };
    }
    case "CHOOSE_PAL": {
      const it = state.palette.items[state.palette.active];
      if (!it) return state;
      let next: AppState = {
        ...state,
        palette: { ...state.palette, open: false },
      };
      if (it.id === "subagents") {
        return withMenuItems({
          ...next,
          view: "session",
          prompt: "/subagents",
          menu: {
            ...next.menu,
            open: true,
            cmd: "subagents",
            step: "target",
            active: 0,
            target: null,
          },
        });
      }
      if (it.id === "broadcast") {
        return toast({ ...next, view: "session" }, "lands in M3/M4");
      }
      return { ...next, view: it.id as ViewName };
    }
    case "START_NEW_AGENT":
      return withMenuItems({
        ...state,
        view: "session",
        prompt: "/new",
        menu: {
          ...state.menu,
          open: true,
          cmd: "new",
          step: "target",
          active: 0,
          target: null,
        },
      });
    case "OPEN_SUBAGENTS_FROM_PAL":
      return withMenuItems({
        ...state,
        view: "session",
        prompt: "/subagents",
        menu: {
          ...state.menu,
          open: true,
          cmd: "subagents",
          step: "target",
          active: 0,
          target: null,
        },
      });
    default:
      return state;
  }
}

type StoreValue = {
  state: AppState;
  dispatch: Dispatch<Action>;
  promptRef: RefObject<HTMLInputElement | null>;
  palInputRef: RefObject<HTMLInputElement | null>;
};

const StoreContext = createContext<StoreValue | null>(null);

async function runSpawn(
  dispatch: Dispatch<Action>,
  state: AppState,
  cmd: string,
  target: string,
  model: string,
) {
  const parent = state.sessions[state.sel];
  const cwd = parent?.cwd || null;
  const harness = harnessLabel(target);
  const before = new Set(
    state.sessions.filter((s) => s.ctl === "tmux" && s.sid).map((s) => s.sid!),
  );
  try {
    const name = await spawnSession(harness, model, cwd);
    dispatch({
      type: "TOAST",
      html: `spawned <b>${esc(name)}</b> · ${esc(harness)} · ${esc(model)}`,
    });
    if (cmd === "subagents") {
      const title = parent?.title || "session";
      const sid = await waitForOwnedSid(before, 15_000);
      if (!sid) {
        dispatch({
          type: "TOAST",
          html: "handoff prompt must be sent manually — mapping timed out",
        });
        return;
      }
      try {
        await sendPrompt(sid, `handoff — ${title}`);
        dispatch({
          type: "TOAST",
          html: `↳ handoff sent to <b>${esc(sid.slice(0, 8))}</b>`,
        });
      } catch (e) {
        dispatch({ type: "TOAST", html: esc(String(e)) });
      }
    }
  } catch (e) {
    dispatch({ type: "TOAST", html: esc(String(e)) });
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const promptRef = useRef<HTMLInputElement | null>(null);
  const palInputRef = useRef<HTMLInputElement | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;
  // TV design-review prototype: sid → last state, to catch red transitions
  const tvPrevRef = useRef<Record<string, string>>({});

  // If the PiP tv is open, a session newly turning red pauses it (design/tv.md).
  // Fire-and-forget; tv_interrupt is a no-op when the tv window is closed.
  function tvOnRed(sessions: Session[]) {
    const prev = tvPrevRef.current;
    const next: Record<string, string> = {};
    for (const s of sessions) {
      const sid = s.sid || s.title;
      next[sid] = s.state;
      if (s.state === "error" && prev[sid] && prev[sid] !== "error") {
        import("@tauri-apps/api/core").then(({ invoke }) =>
          invoke("tv_interrupt", {
            title: s.title,
            detail: "stalled — no output; needs a look",
          }).catch(() => {}),
        );
      }
    }
    tvPrevRef.current = next;
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    (async () => {
      try {
        const wire = await listSessions();
        if (!cancelled) {
          dispatch({
            type: "SET_SESSIONS",
            sessions: wire.map(wireToSession),
          });
        }
      } catch (e) {
        if (!cancelled) {
          dispatch({
            type: "TOAST",
            html: `failed to load sessions: ${esc(String(e))}`,
          });
        }
      }
      try {
        unlisten = await listen<SessionWire[]>("sessions:update", (ev) => {
          const sessions = (ev.payload || []).map(wireToSession);
          tvOnRed(sessions);
          dispatch({ type: "SET_SESSIONS", sessions });
        });
      } catch (e) {
        console.error("listen sessions:update failed", e);
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (state.toasts.show) {
      const t = setTimeout(() => dispatch({ type: "HIDE_TOAST" }), 3800);
      return () => clearTimeout(t);
    }
  }, [state.toasts.show, state.toasts.html]);

  useEffect(() => {
    if (
      state.menu.open &&
      (state.prompt === "/new" || state.prompt === "/subagents")
    ) {
      promptRef.current?.focus();
    }
  }, [state.menu.open, state.prompt]);

  useEffect(() => {
    if (state.palette.open) {
      palInputRef.current?.focus();
    }
  }, [state.palette.open]);

  return (
    <StoreContext.Provider value={{ state, dispatch, promptRef, palInputRef }}>
      {children}
    </StoreContext.Provider>
  );
}

export function useStore() {
  const ctx = useContext(StoreContext);
  if (!ctx) throw new Error("useStore outside StoreProvider");
  return ctx;
}

/** Side-effectful send — called from keyboard Enter. */
export async function doSend(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const s = state.sessions[state.sel];
  if (!s) return;

  if (state.subSel < 0 && s.ctl === "observe") {
    if (!s.sid) {
      dispatch({ type: "TOAST", html: "session has no sid yet" });
      return;
    }
    try {
      const hv = await adoptSession(s.sid);
      dispatch({
        type: "TOAST",
        html: `adopted as ${esc(hv)} — session now runs in the background`,
      });
    } catch (e) {
      dispatch({ type: "TOAST", html: esc(String(e)) });
    }
    return;
  }

  const text = state.prompt.trim();
  if (!text) return;
  dispatch({ type: "CLEAR_PROMPT" });

  if (!s.sid) {
    dispatch({ type: "TOAST", html: "session has no sid yet" });
    return;
  }

  try {
    await sendPrompt(s.sid, text);
    dispatch({ type: "OPTIMISTIC_SENT", i: state.sel, text });
  } catch (e) {
    dispatch({ type: "TOAST", html: esc(String(e)) });
  }
}

/** Choose menu item — may kick off spawn. */
export function chooseMenu(
  state: AppState,
  dispatch: Dispatch<Action>,
): void {
  const it = state.menu.items[state.menu.active];
  if (!it) return;

  if (state.menu.step === "model") {
    const cmd = state.menu.cmd;
    const target = state.menu.target!;
    const model = it.id;
    dispatch({ type: "REQUEST_SPAWN", cmd, target, model });
    void runSpawn(dispatch, state, cmd, target, model);
    return;
  }
  dispatch({ type: "CHOOSE_MENU" });
}
