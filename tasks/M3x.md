# M3x — prove approvals live (closes M3's open acceptance criterion)

**Prerequisite gate:** `claude -p "say ok"` must succeed on this machine.
If it reports OAuth expired, STOP and report — Joe must run `claude /login`
first. Do not build anything else in that case.

**You are doing exactly one thing this session:** run M3's DoD #1 and #2
against a REAL claude code session and record the evidence. The pane parser
was derived from binary strings, GH #11380, and a synthetic fixture — never
from a live dialog (OAuth was expired during M3). Everything downstream
(M7g grammar, both remote channels) routes approvals through this exact
mechanism; this session replaces faith with a capture.

## Definition of done

1. Spawn a claude session via `+ New Agent`; prompt it to run a command it
   must ask about. Within 2s the sidebar row turns red with
   `⏸ wants: <the command>`. `Tab` approves; the TUI proceeds
   (capture-pane proof) and the row leaves red.
2. Repeat; type guidance at the pending approval + ⏎ → the request is
   denied and the guidance visibly arrives in the session (transcript or
   pane proof).
3. The LIVE pane capture is pasted into tasks/M3.md Evidence. If the live
   dialog differs from the fixture (option numbering, wording, layout),
   update `parse_claude_pane` and the test fixtures to match reality;
   unit tests green.
4. Deny timing: verify the 400ms sleep in `approvals::deny` is enough for
   the TUI to return to its input before guidance is typed. If it races,
   replace the sleep with a bounded capture-pane poll (≤2s) for the input
   prompt. Record what you observed either way.
5. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib`.

## Scope fence

- Touch only `approvals.rs` (parser + deny timing) and its tests, plus
  tasks/M3.md Evidence. Nothing else.
- No new features, no UI changes.

## When done

1. Evidence into **tasks/M3.md** (live pane captures, approve + deny
   transcript proof, any parser diffs), and strike the "Open acceptance
   criterion" paragraph in the planner note there.
2. Note the closure on PLAN.md's M3 line (M3 stays ticked — this closes
   its open AC).
3. Refresh `tasks/CURRENT.md` with `tasks/H2.md`.
4. Commit: `M3x: live-proof approvals — pane capture, parser verified against reality`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
