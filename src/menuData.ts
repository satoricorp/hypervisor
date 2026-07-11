/** Command palette / menu static data (was in mockSessions). */

export const HISTORY = [
  {
    when: "jul 08",
    title: "refactor auth middleware to session tokens",
    app: "claude code",
    model: "fable-5",
    num: "812K · $2.04",
    note: "merged pr #214",
  },
  {
    when: "jul 07",
    title: "fix flaky retry test in ci",
    app: "codex",
    model: "gpt-5-codex",
    num: "240K · $0.55",
    note: "landed",
  },
  {
    when: "jul 05",
    title: "migrate dashboard to tailwind v4",
    app: "cursor",
    model: "sonnet-5",
    num: "1.1M · $0",
    note: "included · cursor pro",
  },
  {
    when: "jul 04",
    title: "rewrite queue consumer in go",
    app: "claude code",
    model: "opus-4.8",
    num: "1.1M · $2.84",
    note: "abandoned — kept node impl",
  },
  {
    when: "jul 03",
    title: "add rate limiting to public api",
    app: "codex",
    model: "gpt-5",
    num: "410K · $0.74",
    note: "merged pr #83",
  },
];

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
    id: "loop",
    label: "/loop",
    desc: "keep re-running until you stop it",
  },
  {
    id: "broadcast",
    label: "/broadcast",
    desc: "send the prompt to every live session",
  },
  {
    id: "handoff",
    label: "/handoff",
    desc: "move this session — context and all — to another harness",
  },
  {
    id: "new",
    label: "/new",
    desc: "start a fresh session — pick harness + model",
  },
  {
    id: "worktree",
    label: "/worktree",
    desc: "move this session into a fresh git worktree",
  },
  {
    id: "compact",
    label: "/compact",
    desc: "compact this session’s context window",
  },
  {
    id: "kill",
    label: "/kill",
    desc: "end this session",
  },
];

export const PAL = [
  { id: "session", label: "session", desc: "back to the selected session" },
  {
    id: "history",
    label: "history",
    desc: "finished sessions — searchable, stored locally",
  },
  { id: "usage", label: "usage", desc: "tokens & cost by model" },
  { id: "access", label: "access", desc: "keys & subscriptions" },
  {
    id: "settings",
    label: "settings",
    desc: "notifications · sources · general",
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
    desc: "prompt every live session",
  },
];
