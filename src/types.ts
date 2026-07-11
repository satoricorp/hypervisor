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
  /** Stable session number from backend (M7g). */
  n?: number;
  /** Pending approval letter A–Z (M7g). */
  letter?: string | null;
  loop?: boolean;
  fresh?: boolean;
  log?: LogEntry[];
}

export type ViewName =
  | "session"
  | "usage"
  | "access"
  | "settings"
  | "history"
  | "archived";

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
  label: string;
  detail?: string;
  show: boolean;
}

export interface HealthState {
  watcher: boolean;
  adapters: { harness: string; status: string }[];
  serve: boolean;
}

export interface AppState {
  sessions: Session[];
  /** Adapter session count before the LIMIT=8 display cap. */
  total: number;
  sel: number;
  subSel: number;
  view: ViewName;
  menu: MenuState;
  palette: PaletteState;
  yolo: boolean;
  toasts: ToastState;
  health: HealthState;
  prompt: string;
  historyFilter: string;
}
