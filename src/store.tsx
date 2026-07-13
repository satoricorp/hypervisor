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
import {
  adoptSession,
  approveSession,
  archiveIdle,
  archiveSession,
  broadcastPrompt,
  compactSession,
  denySession,
  getYolo,
  killSession,
  listSessions,
  renameSession,
  sendPrompt,
  setYolo,
  spawnSession,
  waitForOwnedSid,
} from "./api";
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
    const idleCount = state.sessions.filter(
      (s) => s.state === "done" || s.state === "error",
    ).length;
    const sel = state.sessions[state.sel];
    return ROOT_CMDS.filter((c) => {
      // /compact only for claude tmux sessions.
      if (c.id === "compact") {
        if (sel?.ctl !== "tmux") return false;
        if (sel.app !== "claude code" && sel.app !== "claude") return false;
      }
      // /kill only for owned tmux.
      if (c.id === "kill" && sel?.ctl !== "tmux") return false;
      // /worktree needs a selected session in a spawnable repo.
      if (c.id === "worktree") {
        if (!sel?.cwd) return false;
        const app = sel.app;
        if (
          app !== "claude code" &&
          app !== "claude" &&
          app !== "codex" &&
          app !== "opencode"
        )
          return false;
      }
      const label = c.label.slice(1).toLowerCase(); // without leading /
      // Trailing-text commands: match prefix word so `/rename foo` still hits.
      if (c.id === "rename") {
        return f === "rename" || f.startsWith("rename ");
      }
      if (c.id === "plan") {
        return f === "plan" || f.startsWith("plan ");
      }
      if (c.id === "broadcast") {
        return f === "broadcast" || f.startsWith("broadcast ");
      }
      return label.startsWith(f);
    }).map((c) => {
      if (c.id === "archive-idle") {
        return {
          ...c,
          desc:
            idleCount === 0
              ? "no idle sessions to hide"
              : `hide ${idleCount} done/stalled session${idleCount === 1 ? "" : "s"}`,
        };
      }
      if (c.id === "rename") {
        const arg = prompt.replace(/^\/rename\s*/i, "");
        return {
          ...c,
          desc: arg
            ? `rename to “${arg}”`
            : 'usage: /rename <title> · "/rename -" reverts',
        };
      }
      if (c.id === "plan") {
        const arg = prompt.replace(/^\/plan\s*/i, "");
        return {
          ...c,
          desc: arg
            ? `plan: ${arg.slice(0, 60)}${arg.length > 60 ? "…" : ""}`
            : "usage: /plan <prompt> — parks execution until you approve",
        };
      }
      if (c.id === "broadcast") {
        const arg = prompt.replace(/^\/broadcast\s*/i, "");
        return {
          ...c,
          desc: arg
            ? `broadcast to controllable sessions`
            : "usage: /broadcast <prompt>",
        };
      }
      return c;
    });
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
    total: 0,
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
    toasts: { label: "", show: false },
    health: { watcher: false, adapters: [], serve: false },
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
  | { type: "TOAST"; label: string; detail?: string }
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
  | { type: "SET_YOLO_SILENT"; on: boolean }
  | { type: "SET_SESSIONS"; sessions: Session[]; total: number }
  | {
      type: "SET_HEALTH";
      health: {
        watcher: boolean;
        adapters: { harness: string; status: string }[];
        serve: boolean;
      };
    }
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

function toast(
  state: AppState,
  label: string,
  detail?: string,
): AppState {
  return { ...state, toasts: { label, detail, show: true } };
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
      return toast(state, action.label, action.detail);
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
      // Backend sync happens in Statusbar / doSetYolo; reducer updates UI.
      return toast(
        { ...state, yolo: action.on },
        action.on
          ? "yolo on — auto-approving everything. godspeed."
          : "yolo off — approvals wait for ⇥ again",
      );
    case "SET_YOLO_SILENT":
      return { ...state, yolo: action.on };
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
      return { ...state, sessions, total: action.total, sel };
    }
    case "SET_HEALTH":
      return { ...state, health: action.health };
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
      // Side-effect in doApprove; reducer is a no-op placeholder.
      return state;
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
          it.id === "broadcast" ||
          it.id === "kill" ||
          it.id === "compact"
        ) {
          // Side effect in chooseMenu.
          return next;
        }
        if (it.id === "archive" || it.id === "archive-idle") {
          // Side effect in chooseMenu → doArchive / doArchiveIdle.
          return next;
        }
        if (it.id === "rename") {
          // Side effect in chooseMenu → doRename (needs trailing text).
          return next;
        }
        return toast(next, `/${it.id}`, "not wired");
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
        return toast(
          { ...next, view: "session" },
          "usage: /broadcast <prompt>",
          "type it in the prompt bar",
        );
      }
      if (it.id === "tv") {
        // side effect (window toggle) happens at the dispatch sites
        return next;
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
  promptRef: RefObject<HTMLTextAreaElement | null>;
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
    const name = await spawnSession(
      harness,
      model,
      cwd,
      cmd === "subagents" ? "subagents" : "new",
    );
    dispatch({
      type: "TOAST",
      label: name,
      detail: `${harness} · ${model}`,
    });
    if (cmd === "subagents") {
      const title = parent?.title || "session";
      const sid = await waitForOwnedSid(before, 15_000);
      if (!sid) {
        dispatch({
          type: "TOAST",
          label: "handoff timed out",
          detail: "prompt must be sent manually",
        });
        return;
      }
      try {
        await sendPrompt(sid, `handoff — ${title}`);
        dispatch({
          type: "TOAST",
          label: `handoff → ${sid.slice(0, 8)}`,
        });
      } catch (e) {
        dispatch({ type: "TOAST", label: String(e) });
      }
    }
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const promptRef = useRef<HTMLTextAreaElement | null>(null);
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
      if (
        (s.state === "error" || s.state === "input") &&
        prev[sid] &&
        prev[sid] !== "error" &&
        prev[sid] !== "input"
      ) {
        const detail = s.approval
          ? `⏸ wants: ${s.approval}`
          : "stalled — no output; needs a look";
        import("@tauri-apps/api/core").then(({ invoke }) =>
          invoke("tv_interrupt", {
            title: s.title,
            detail,
          }).catch(() => {}),
        );
      }
    }
    tvPrevRef.current = next;
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    let unlistenToast: (() => void) | undefined;
    let unlistenHealth: (() => void) | undefined;

    (async () => {
      try {
        const [update, yolo] = await Promise.all([listSessions(), getYolo()]);
        if (!cancelled) {
          dispatch({
            type: "SET_SESSIONS",
            sessions: update.sessions.map(wireToSession),
            total: update.total,
          });
          if (yolo) dispatch({ type: "SET_YOLO_SILENT", on: true });
        }
      } catch (e) {
        if (!cancelled) {
          dispatch({
            type: "TOAST",
            label: "failed to load sessions",
            detail: String(e),
          });
        }
      }
      try {
        unlisten = await listen<{ sessions: SessionWire[]; total: number }>(
          "sessions:update",
          (ev) => {
            const payload = ev.payload || { sessions: [], total: 0 };
            const sessions = (payload.sessions || []).map(wireToSession);
            tvOnRed(sessions);
            dispatch({
              type: "SET_SESSIONS",
              sessions,
              total: payload.total ?? sessions.length,
            });
          },
        );
      } catch (e) {
        console.error("listen sessions:update failed", e);
      }
      try {
        unlistenToast = await listen<{
          label: string;
          detail?: string | null;
        }>("toast", (ev) => {
          if (ev.payload?.label) {
            dispatch({
              type: "TOAST",
              label: ev.payload.label,
              detail: ev.payload.detail ?? undefined,
            });
          }
        });
      } catch (e) {
        console.error("listen toast failed", e);
      }
      try {
        unlistenHealth = await listen<{
          watcher: boolean;
          adapters: { harness: string; status: string }[];
          serve: boolean;
        }>("health", (ev) => {
          if (ev.payload) {
            dispatch({ type: "SET_HEALTH", health: ev.payload });
          }
        });
      } catch (e) {
        console.error("listen health failed", e);
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
      unlistenToast?.();
      unlistenHealth?.();
    };
  }, []);

  useEffect(() => {
    if (state.toasts.show) {
      const t = setTimeout(() => dispatch({ type: "HIDE_TOAST" }), 3800);
      return () => clearTimeout(t);
    }
  }, [state.toasts.show, state.toasts.label, state.toasts.detail]);

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

/** Tab → approve selected session's pending permission. */
export async function doApprove(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({
      type: "TOAST",
      label: "nothing pending approval on this session",
    });
    return;
  }
  if (!s.approval) {
    dispatch({
      type: "TOAST",
      label: "nothing pending approval on this session",
    });
    return;
  }
  try {
    await approveSession(s.sid);
    dispatch({
      type: "TOAST",
      label: "approved",
      detail: s.approval,
    });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** Statusbar yolo toggle — sync backend + UI. */
export async function doSetYolo(
  on: boolean,
  dispatch: Dispatch<Action>,
): Promise<void> {
  try {
    await setYolo(on);
    dispatch({ type: "SET_YOLO", on });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
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
      dispatch({ type: "TOAST", label: "session has no sid yet" });
      return;
    }
    try {
      const hv = await adoptSession(s.sid);
      dispatch({
        type: "TOAST",
        label: `adopted as ${hv}`,
        detail: "session now runs in the background",
      });
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
    }
    return;
  }

  const text = state.prompt.trim();
  if (!text) return;

  // Typing at a pending approval = deny with guidance.
  if (state.subSel < 0 && s.approval && s.sid) {
    dispatch({ type: "CLEAR_PROMPT" });
    try {
      await denySession(s.sid, text);
      dispatch({
        type: "TOAST",
        label: "denied with guidance",
        detail: text,
      });
      dispatch({ type: "OPTIMISTIC_SENT", i: state.sel, text });
    } catch (e) {
      dispatch({ type: "TOAST", label: String(e) });
    }
    return;
  }

  dispatch({ type: "CLEAR_PROMPT" });

  if (!s.sid) {
    dispatch({ type: "TOAST", label: "session has no sid yet" });
    return;
  }

  try {
    await sendPrompt(s.sid, text);
    dispatch({ type: "OPTIMISTIC_SENT", i: state.sel, text });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** Choose menu item — may kick off spawn, archive, or REALIZE commands. */
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
  if (state.menu.step === "root") {
    if (it.id === "archive") {
      dispatch({ type: "CHOOSE_MENU" });
      void doArchive(state, dispatch);
      return;
    }
    if (it.id === "archive-idle") {
      dispatch({ type: "CHOOSE_MENU" });
      void doArchiveIdle(dispatch);
      return;
    }
    if (it.id === "rename") {
      const arg = state.prompt.replace(/^\/rename\s*/i, "").trim();
      dispatch({ type: "CHOOSE_MENU" });
      void doRename(state, dispatch, arg);
      return;
    }
    if (it.id === "plan") {
      const arg = state.prompt.replace(/^\/plan\s*/i, "").trim();
      dispatch({ type: "CHOOSE_MENU" });
      void doPlan(state, dispatch, arg);
      return;
    }
    if (it.id === "broadcast") {
      const arg = state.prompt.replace(/^\/broadcast\s*/i, "").trim();
      dispatch({ type: "CHOOSE_MENU" });
      void doBroadcast(dispatch, arg);
      return;
    }
    if (it.id === "review") {
      dispatch({ type: "CHOOSE_MENU" });
      void doReview(state, dispatch);
      return;
    }
    if (it.id === "kill") {
      dispatch({ type: "CHOOSE_MENU" });
      void doKill(state, dispatch);
      return;
    }
    if (it.id === "compact") {
      dispatch({ type: "CHOOSE_MENU" });
      void doCompact(state, dispatch);
      return;
    }
    if (it.id === "worktree") {
      dispatch({ type: "CHOOSE_MENU" });
      void doWorktree(state, dispatch);
      return;
    }
  }
  dispatch({ type: "CHOOSE_MENU" });
}

const PLAN_PREFIX =
  "Plan first — do not execute any tools or make any changes until I approve the plan. Write a concrete plan for:\n\n";

const REVIEW_PROMPT =
  "Review the current git diff in this working directory. Focus on bugs, regressions, and missing tests. Do not modify files unless asked.";

/** /plan <prompt> — prefix + send; M3 approval flow parks tool execution. */
export async function doPlan(
  state: AppState,
  dispatch: Dispatch<Action>,
  arg: string,
): Promise<void> {
  if (!arg) {
    dispatch({
      type: "TOAST",
      label: "usage: /plan <prompt>",
    });
    return;
  }
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({ type: "TOAST", label: "no session selected" });
    return;
  }
  if (s.ctl === "observe") {
    dispatch({
      type: "TOAST",
      label: "adopt first",
      detail: "observe-only — press ⏎ to adopt",
    });
    return;
  }
  const text = PLAN_PREFIX + arg;
  try {
    await sendPrompt(s.sid, text);
    dispatch({ type: "OPTIMISTIC_SENT", i: state.sel, text });
    dispatch({
      type: "TOAST",
      label: "plan requested",
      detail: "execution waits on your approval",
    });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /broadcast <prompt> — every controllable session. */
export async function doBroadcast(
  dispatch: Dispatch<Action>,
  arg: string,
): Promise<void> {
  if (!arg) {
    dispatch({ type: "TOAST", label: "usage: /broadcast <prompt>" });
    return;
  }
  try {
    const results = await broadcastPrompt(arg);
    const ok = results.filter((r) => r.ok).length;
    const fail = results.length - ok;
    dispatch({
      type: "TOAST",
      label: `broadcast ${ok} ok${fail ? ` · ${fail} failed` : ""}`,
      detail: results
        .map((r) => `${r.title.slice(0, 24)}: ${r.ok ? "ok" : r.detail}`)
        .join(" · "),
    });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /worktree — spawn a fresh agent in a new git worktree of the selected repo. */
export async function doWorktree(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const sel = state.sessions[state.sel];
  const cwd = sel?.cwd || null;
  if (!cwd) {
    dispatch({ type: "TOAST", label: "select a session in a repo first" });
    return;
  }
  const app = sel.app === "claude" ? "claude code" : sel.app;
  const harness =
    app === "codex" || app === "opencode" || app === "claude code"
      ? app
      : "claude code";
  const id = harness === "claude code" ? "claude" : harness;
  const model =
    sel.model && sel.model !== "—" ? sel.model : (MODELS[id]?.[0] ?? "");
  try {
    const name = await spawnSession(harness, model, cwd, "new", true);
    dispatch({ type: "TOAST", label: name, detail: "worktree spawning…" });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /review — spawn a reviewer in the parent's cwd with a canned prompt. */
export async function doReview(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const parent = state.sessions[state.sel];
  const cwd = parent?.cwd || null;
  const harness = "claude code";
  const model = "claude-sonnet-5";
  const before = new Set(
    state.sessions.filter((s) => s.ctl === "tmux" && s.sid).map((s) => s.sid!),
  );
  try {
    const name = await spawnSession(harness, model, cwd);
    dispatch({
      type: "TOAST",
      label: name,
      detail: "reviewer spawning…",
    });
    const sid = await waitForOwnedSid(before, 15_000);
    if (!sid) {
      dispatch({
        type: "TOAST",
        label: "reviewer timed out",
        detail: "send the review prompt manually",
      });
      return;
    }
    await sendPrompt(sid, REVIEW_PROMPT);
    dispatch({
      type: "TOAST",
      label: `review → ${sid.slice(0, 8)}`,
    });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /kill — tmux kill-session + owned.json cleanup. */
export async function doKill(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({ type: "TOAST", label: "nothing to kill" });
    return;
  }
  try {
    const msg = await killSession(s.sid);
    dispatch({ type: "TOAST", label: msg });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /compact — claude tmux only. */
export async function doCompact(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({ type: "TOAST", label: "nothing to compact" });
    return;
  }
  try {
    const msg = await compactSession(s.sid);
    dispatch({ type: "TOAST", label: msg });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /rename <title> — local override; "-" or empty clears. */
export async function doRename(
  state: AppState,
  dispatch: Dispatch<Action>,
  arg: string,
): Promise<void> {
  if (!arg) {
    dispatch({
      type: "TOAST",
      label: 'usage: /rename <title> · "/rename -" reverts to the derived title',
    });
    return;
  }
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({ type: "TOAST", label: "nothing to rename" });
    return;
  }
  try {
    const msg = await renameSession(s.sid, arg);
    dispatch({ type: "TOAST", label: msg });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** ⌘⌫ / /archive — hide the selected session. */
export async function doArchive(
  state: AppState,
  dispatch: Dispatch<Action>,
): Promise<void> {
  const s = state.sessions[state.sel];
  if (!s?.sid) {
    dispatch({ type: "TOAST", label: "nothing to archive" });
    return;
  }
  try {
    const msg = await archiveSession(s.sid);
    dispatch({ type: "TOAST", label: msg });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}

/** /archive idle — tombstone every done/stalled row. */
export async function doArchiveIdle(
  dispatch: Dispatch<Action>,
): Promise<void> {
  try {
    const n = await archiveIdle();
    dispatch({
      type: "TOAST",
      label: n === 0 ? "nothing idle to archive" : `archived ${n}`,
    });
  } catch (e) {
    dispatch({ type: "TOAST", label: String(e) });
  }
}
