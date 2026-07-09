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
import { MOCK_SESSIONS, MODELS, PAL, ROOT_CMDS, TARGETS } from "./mockSessions";
import { ensureLog } from "./constants";
import type {
  AppState,
  MenuItem,
  Session,
  ViewName,
} from "./types";
import {
  schedulePlanReady,
  scheduleSessionComplete,
  scheduleSubPromptDone,
  scheduleSubagentDone,
  scheduleYoloApprovals,
  startThinkingCycler,
  stopThinkingCycler,
} from "./simulate";

function deepCloneSessions(sessions: Session[]): Session[] {
  return structuredClone(sessions);
}

function menuItemsFor(
  state: AppState,
): MenuItem[] {
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
    sessions: deepCloneSessions(MOCK_SESSIONS),
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
  | { type: "COMPLETE_SESSION"; i: number }
  | { type: "SUB_DONE"; sessionIndex: number; subIndex: number }
  | { type: "PLAN_READY"; i: number }
  | { type: "THINK_TICK" }
  | { type: "SEND" }
  | { type: "APPROVE_SEL" }
  | { type: "YOLO_APPROVE"; i: number }
  | { type: "CHOOSE_MENU" }
  | { type: "CHOOSE_PAL" }
  | { type: "START_NEW_AGENT" }
  | { type: "OPEN_SUBAGENTS_FROM_PAL" }
  | { type: "PATCH_SESSION"; i: number; patch: Partial<Session> };

function toast(state: AppState, html: string): AppState {
  return { ...state, toasts: { html, show: true } };
}

function select(state: AppState, i: number): AppState {
  const len = state.sessions.length;
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
  if (!s.subs.length) return state;
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

function completeSession(state: AppState, i: number): AppState {
  return toast(
    mutateSessions(state, (sessions) => {
      const s = sessions[i];
      s.state = "done";
      s.age = "now";
      s.result = "✓ done — response ready to review";
      s.output = [];
    }),
    `<b>●</b> ${esc(state.sessions[i].app)} responded — ${esc(state.sessions[i].title)}`,
  );
}

function esc(s: string): string {
  return String(s).replace(
    /[&<>"]/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string,
  );
}

function sendTo(state: AppState, i: number, text: string): AppState {
  return mutateSessions(state, (sessions) => {
    const s = sessions[i];
    if (s.fresh) {
      s.title = text.length > 64 ? text.slice(0, 61) + "…" : text;
      s.fresh = false;
    }
    if (s.log) {
      if (s.state === "done" && s.result) {
        s.log.push({ k: "agent", t: s.result });
        (s.output || []).forEach((l) => s.log!.push({ k: "agent", t: l }));
      }
      s.log.push({ k: "you", t: text });
    }
    s.sent = text;
    s.state = "working";
    s.output = [];
    s.tool = "Read";
    s.toolArg = "session context";
    s.think = [
      "reading the new instruction…",
      "scanning recent changes…",
      "planning the next step…",
    ];
    s.thinkIdx = 0;
  });
}

function approve(state: AppState, i: number): { state: AppState; ok: boolean; tool?: string; toolArg?: string } {
  const s0 = state.sessions[i];
  if (!s0.approval) return { state, ok: false };
  const cmd = s0.approval;
  const next = mutateSessions(state, (sessions) => {
    const s = sessions[i];
    s.approval = null;
    ensureLog(s);
    s.log!.push({ k: "tool", t: `approved → ${cmd}` });
    s.state = "working";
    s.tool = "Bash";
    s.toolArg = cmd.replace(/^Bash\((.*)\)$/, "$1");
    s.think = [
      "running the approved command…",
      "watching output for failures",
      "will report when finished",
    ];
    s.thinkIdx = 0;
  });
  return {
    state: next,
    ok: true,
    tool: "Bash",
    toolArg: cmd.replace(/^Bash\((.*)\)$/, "$1"),
  };
}

function spawnSub(
  state: AppState,
  target: string,
  model: string,
  task?: string,
): AppState {
  const title = state.sessions[state.sel].title;
  return toast(
    mutateSessions(state, (sessions) => {
      const s = sessions[state.sel];
      s.subs.push({
        target,
        model,
        state: "working",
        task: task || "handoff — " + s.title,
      });
    }),
    `↳ spawned <b>${esc(target)} · ${esc(model)}</b> ← “${esc(title)}”`,
  );
}

function createSession(
  state: AppState,
  target: string,
  model: string,
): AppState {
  const app = target === "claude" ? "claude code" : target;
  const busy = state.sessions.some((x) => x.repo === "gx");
  const wt = busy ? "wt-" + (2 + Math.floor(Math.random() * 7)) : undefined;
  const sessions = deepCloneSessions(state.sessions);
  sessions.push({
    app,
    model,
    title: "new session — send the first prompt",
    sent: "—",
    state: "input",
    ask: "? waiting for the first prompt",
    ctl: "tmux",
    repo: "gx",
    br: "main",
    wt,
    subs: [],
    fresh: true,
    log: [
      { k: "tool", t: "History(2 session summaries attached)" },
      {
        k: "agent",
        t: "auth-tokens: “sessions moved to redis-backed tokens, ttl 24h, logout clears both stores”",
      },
      {
        k: "agent",
        t: "sync-backoff: “retries capped at 30s with full jitter to avoid thundering herd”",
      },
    ],
  });
  const next = select({ ...state, sessions }, sessions.length - 1);
  return toast(
    next,
    (wt
      ? `session created in <b>gx</b> — repo busy, spun up worktree <b>${wt}</b>`
      : "session created in <b>gx</b>") +
      " · context from 2 prior sessions attached",
  );
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
    case "COMPLETE_SESSION":
      return completeSession(state, action.i);
    case "SUB_DONE":
      return mutateSessions(state, (sessions) => {
        const sub = sessions[action.sessionIndex]?.subs[action.subIndex];
        if (sub) sub.state = "done";
      });
    case "PLAN_READY":
      return toast(
        mutateSessions(state, (sessions) => {
          const s = sessions[action.i];
          s.state = "input";
          s.ask = "? plan ready — 6 steps, est. 40m. approve to execute?";
        }),
        `plan ready — <b>${esc(state.sessions[action.i].title)}</b> waiting on approval`,
      );
    case "THINK_TICK":
      return mutateSessions(state, (sessions) => {
        sessions.forEach((s) => {
          if (s.state === "working" && s.think && s.think.length) {
            s.thinkIdx = ((s.thinkIdx ?? 0) + 1) % s.think.length;
          }
        });
      });
    case "SEND": {
      const s = state.sessions[state.sel];
      if (state.subSel < 0 && s.ctl === "observe") {
        if (s.noAdopt) {
          return toast(
            state,
            "no control path for claude.ai yet — browser extension planned",
          );
        }
        return toast(
          mutateSessions(state, (sessions) => {
            const sess = sessions[state.sel];
            sess.ctl = "tmux";
            ensureLog(sess);
            sess.log!.push({
              k: "tool",
              t: `tmux new-session -d -s hv-${state.sel + 1} 'claude --resume ${sess.sid || "…"}'`,
            });
          }),
          `adopted as <b>hv-${state.sel + 1}</b> — now running in the background, keeps working with the lid closed`,
        );
      }
      const text = state.prompt.trim();
      if (!text) return state;
      let next = state;
      if (state.subSel < 0 && s.approval) {
        next = toast(
          mutateSessions(next, (sessions) => {
            const sess = sessions[state.sel];
            sess.approval = null;
            ensureLog(sess);
            sess.log!.push({ k: "tool", t: "denied — guidance sent instead" });
          }),
          "denied — your guidance goes to the session",
        );
      }
      if (state.subSel >= 0 && s.subs[state.subSel]) {
        next = mutateSessions(next, (sessions) => {
          const x = sessions[state.sel].subs[state.subSel];
          if (x.log) x.log.push({ k: "you", t: text });
          x.task = text;
          x.state = "working";
        });
      } else {
        next = sendTo(next, state.sel, text);
      }
      return { ...next, prompt: "" };
    }
    case "APPROVE_SEL": {
      const result = approve(state, state.sel);
      if (result.ok) {
        return toast(
          result.state,
          `⇥ approved — ${esc(result.tool!)}(${esc(result.toolArg!)})`,
        );
      }
      return toast(state, "nothing pending approval on this session");
    }
    case "YOLO_APPROVE": {
      if (!state.yolo) return state;
      const result = approve(state, action.i);
      return result.ok ? result.state : state;
    }
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
        let next: AppState = {
          ...state,
          menu: { ...state.menu, open: false },
          prompt: "",
        };
        if (it.id === "broadcast") {
          next = toast(next, "prompt queued to all live sessions");
          next.sessions.forEach((s, i) => {
            if (s.state !== "error" && s.ctl !== "observe") {
              next = sendTo(next, i, "broadcast: status check — summarize where you are");
            }
          });
          return next;
        }
        if (it.id === "plan") {
          next = toast(
            mutateSessions(next, (sessions) => {
              const s = sessions[state.sel];
              s.state = "working";
              s.tool = "Plan";
              s.toolArg = "drafting approach";
              s.think = [
                "breaking the task into ordered steps",
                "checking constraints against the codebase",
                "estimating scope per step",
              ];
              s.thinkIdx = 0;
            }),
            `planning “${esc(state.sessions[state.sel].title)}” — will pause for approval`,
          );
          return next;
        }
        if (it.id === "review") {
          return spawnSub(
            next,
            "codex",
            "gpt-5-codex",
            "review the diff — correctness, tests, edge cases",
          );
        }
        if (it.id === "loop") {
          const wasLoop = !!state.sessions[state.sel].loop;
          return toast(
            mutateSessions(next, (sessions) => {
              sessions[state.sel].loop = !wasLoop;
            }),
            wasLoop
              ? "loop stopped"
              : `↻ looping “${esc(state.sessions[state.sel].title)}” every 10m until stopped`,
          );
        }
        if (it.id === "worktree") {
          const s = state.sessions[state.sel];
          if (!s.repo) {
            return toast(
              next,
              "this session has no git workspace — /worktree needs a repo",
            );
          }
          const wt = "wt-" + (2 + Math.floor(Math.random() * 7));
          return toast(
            mutateSessions(next, (sessions) => {
              sessions[state.sel].wt = wt;
            }),
            `↳ “${esc(s.title)}” moved to fresh worktree <b>${wt}</b> — branch preserved`,
          );
        }
        return toast(
          next,
          `<b>/${esc(it.id)}</b> — concept only, not wired in this mock`,
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
      {
        const closed = {
          ...state,
          menu: { ...state.menu, open: false },
          prompt: "",
        };
        if (state.menu.cmd === "new") {
          return createSession(closed, state.menu.target!, it.id);
        }
        return spawnSub(closed, state.menu.target!, it.id);
      }
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
        next = toast(
          { ...next, view: "session" },
          "prompt queued to all live sessions",
        );
        next.sessions.forEach((s, i) => {
          if (s.state !== "error" && s.ctl !== "observe") {
            next = sendTo(
              next,
              i,
              "broadcast: status check — summarize where you are",
            );
          }
        });
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
    case "PATCH_SESSION":
      return mutateSessions(state, (sessions) => {
        Object.assign(sessions[action.i], action.patch);
      });
    default:
      return state;
  }
}

// Side-effect bridge: schedule timers after certain actions.
type StoreValue = {
  state: AppState;
  dispatch: Dispatch<Action>;
  promptRef: RefObject<HTMLInputElement | null>;
  palInputRef: RefObject<HTMLInputElement | null>;
};

const StoreContext = createContext<StoreValue | null>(null);

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const promptRef = useRef<HTMLInputElement | null>(null);
  const palInputRef = useRef<HTMLInputElement | null>(null);
  const prevRef = useRef(state);
  const yoloRef = useRef(state.yolo);

  useEffect(() => {
    yoloRef.current = state.yolo;
  }, [state.yolo]);

  useEffect(() => {
    startThinkingCycler(() => dispatch({ type: "THINK_TICK" }));
    return () => stopThinkingCycler();
  }, []);

  useEffect(() => {
    if (state.toasts.show) {
      const t = setTimeout(() => dispatch({ type: "HIDE_TOAST" }), 3800);
      return () => clearTimeout(t);
    }
  }, [state.toasts.show, state.toasts.html]);

  // Detect transitions that need timers
  useEffect(() => {
    const prev = prevRef.current;
    prevRef.current = state;

    // Session went working → schedule complete
    state.sessions.forEach((s, i) => {
      const p = prev.sessions[i];
      if (s.state === "working" && (!p || p.state !== "working" || p.sent !== s.sent || p.tool !== s.tool)) {
        // Plan has its own timer
        if (s.tool === "Plan") {
          schedulePlanReady(i, (idx) =>
            dispatch({ type: "PLAN_READY", i: idx }),
          );
        } else if (s.tool === "Bash" || s.tool === "Read" || s.tool === "Edit" || s.tool === "Grep" || s.tool === "WebSearch") {
          // Only schedule for newly-started work from send/approve, not initial mock working
          if (p && (p.state !== "working" || p.sent !== s.sent || (p.approval && !s.approval))) {
            scheduleSessionComplete(i, (idx) =>
              dispatch({ type: "COMPLETE_SESSION", i: idx }),
            );
          }
        }
      }
    });

    // New subagent spawned
    state.sessions.forEach((s, i) => {
      const p = prev.sessions[i];
      if (!p) return;
      if (s.subs.length > p.subs.length) {
        const subIndex = s.subs.length - 1;
        scheduleSubagentDone(
          (si, sj) => dispatch({ type: "SUB_DONE", sessionIndex: si, subIndex: sj }),
          i,
          subIndex,
        );
      } else {
        s.subs.forEach((sub, j) => {
          const ps = p.subs[j];
          if (ps && sub.state === "working" && ps.state !== "working") {
            scheduleSubPromptDone(
              (si, sj) =>
                dispatch({ type: "SUB_DONE", sessionIndex: si, subIndex: sj }),
              i,
              j,
            );
          }
        });
      }
    });

    // Yolo turned on
    if (state.yolo && !prev.yolo) {
      scheduleYoloApprovals(state.sessions.length, (i) => {
        if (yoloRef.current) dispatch({ type: "YOLO_APPROVE", i });
      });
    }
  }, [state]);

  // Focus prompt when starting new agent / subagents from palette
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
