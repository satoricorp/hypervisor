/** Command palette / menu static data. */

export const TARGETS = [
  { id: "opencode", label: "opencode", desc: "local cli · any provider" },
  { id: "codex", label: "codex", desc: "openai · terminal" },
  { id: "claude", label: "claude code", desc: "anthropic · terminal" },
  { id: "cursor", label: "cursor agent", desc: "ide background agent" },
];

export const MODELS: Record<string, string[]> = {
  opencode: [
    "opencode/big-pickle",
    "opencode/nemotron-3-ultra-free",
    "openai/gpt-5",
    "openai/gpt-5-mini",
  ],
  codex: ["gpt-5-codex", "gpt-5", "o4-mini"],
  claude: [
    "claude-fable-5",
    "claude-opus-4-8",
    "claude-sonnet-5",
    "claude-haiku-4.5",
  ],
  cursor: ["composer-2", "claude-sonnet-5", "gpt-5"],
};

export const ROOT_CMDS = [
  {
    id: "subagents",
    label: "/subagents",
    desc: "hand this session’s work to another app + model",
  },
  {
    id: "plan",
    label: "/plan",
    desc: "draft a plan first — executes only after you approve",
  },
  {
    id: "review",
    label: "/review",
    desc: "spawn a reviewer on this session’s diff",
  },
  {
    id: "broadcast",
    label: "/broadcast",
    desc: "send the prompt to every live session",
  },
  {
    id: "new",
    label: "/new",
    desc: "start a fresh session — pick harness + model",
  },
  {
    id: "worktree",
    label: "/worktree",
    desc: "new agent in a fresh git worktree of this repo",
  },
  {
    id: "compact",
    label: "/compact",
    desc: "compact this session’s context window (claude tmux)",
  },
  {
    id: "kill",
    label: "/kill",
    desc: "end this session",
  },
  {
    id: "archive",
    label: "/archive",
    desc: "hide selected session from the board",
  },
  {
    id: "archive-idle",
    label: "/archive idle",
    desc: "hide every done/stalled session",
  },
  {
    id: "rename",
    label: "/rename",
    desc: "set a custom title · /rename - reverts",
  },
];

export const PAL = [
  { id: "session", label: "session", desc: "back to the selected session" },
  {
    id: "history",
    label: "history",
    desc: "older + archived sessions — searchable",
  },
  {
    id: "archived",
    label: "archived",
    desc: "hidden sessions — unarchive to restore",
  },
  { id: "usage", label: "usage", desc: "live session counts by harness" },
  { id: "access", label: "access", desc: "keys & subscriptions (presence)" },
  {
    id: "settings",
    label: "settings",
    desc: "sources · tv · launch at login",
  },
  { id: "tv", label: "tv", desc: "picture-in-picture youtube (⌘T)" },
  {
    id: "subagents",
    label: "/subagents",
    desc: "hand off from the selected session",
  },
  {
    id: "broadcast",
    label: "/broadcast",
    desc: "type /broadcast <prompt> in the bar",
  },
];
