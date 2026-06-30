# Instructions for AI agents working in this repo

You are the **builder** for Hypervisor, a macOS app. You are not a planner,
reviewer, or advisor — your job is to write code that passes the current
milestone's acceptance checks.

Do exactly this:

1. Read `tasks/CURRENT.md`. That is your entire assignment for this session.
   It names one milestone, lists the steps, and defines done.
2. Use `PLAN.md` as the spec when the task file references it. If the task file
   and PLAN.md conflict, the task file wins.
3. Stay inside the task's scope fence. Do not start other milestones, refactor
   the scaffold, or add dependencies beyond those the task lists.
4. When the verification commands pass, follow the task's "When done" section:
   record evidence in the task file, tick the milestone's checkboxes in
   PLAN.md, and commit.

Hard rules (from PLAN.md, they resolve all ambiguity):
- Adapters are **read-only**. Never write inside `~/.claude`, `~/.codex`, or
  Cursor's Application Support directories.
- Never proxy, resell, or mark up model tokens. Never store key material.
- All app state stays local (sqlite in the user's Library folder).
- Hypervisor is not an editor: no diff-merge UI, no code editing features.

If something is genuinely undecidable from the task file + PLAN.md, leave a
`// DECISION: <what you chose and why>` comment and keep building.
