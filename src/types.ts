/** UI session types for M0. Mockup field names (camelCase). M2 maps Rust snake_case. */

export type SessionState = "working" | "done" | "input" | "error";

export type ControlTier = "tmux" | "api" | "native" | "watch" | "observe";

export type LogKind = "you" | "agent" | "tool";

export interface LogEntry {
  k: LogKind;
  t: string;
}

export interface Subagent {
  target: string;
  model: string;
  state: SessionState;
  task: string;
  log?: LogEntry[];
}

export interface Session {
  app: string;
  model: string;
  title: string;
  sent: string;
  state: SessionState;
  ctl: ControlTier;
  tool?: string;
  toolArg?: string;
  think?: string[];
  thinkIdx?: number;
  repo?: string;
  br?: string;
  wt?: string;
  subs: Subagent[];
  approval?: string | null;
  ask?: string;
  fail?: string;
  result?: string;
  output?: string[];
  age?: string;
  noAdopt?: boolean;
  sid?: string;
  cwd?: string;
  sidechains?: number;
  loop?: boolean;
  fresh?: boolean;
  log?: LogEntry[];
}

export type ViewName =
  | "session"
  | "usage"
  | "access"
  | "settings"
  | "history";

export type MenuStep = "root" | "target" | "model";

export interface MenuState {
  open: boolean;
  step: MenuStep;
  cmd: string;
  target: string | null;
  active: number;
  items: MenuItem[];
}

export interface MenuItem {
  id: string;
  label: string;
  desc?: string;
}

export interface PaletteState {
  open: boolean;
  active: number;
  filter: string;
  items: MenuItem[];
}

export interface ToastState {
  html: string;
  show: boolean;
}

export interface AppState {
  sessions: Session[];
  sel: number;
  subSel: number;
  view: ViewName;
  menu: MenuState;
  palette: PaletteState;
  yolo: boolean;
  toasts: ToastState;
  prompt: string;
  historyFilter: string;
}
