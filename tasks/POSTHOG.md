# POSTHOG — content-free product analytics

**Prerequisite gates:** REALIZE ticked ✓, AND the three Joe inputs below
present in this file's "Inputs" section. If inputs are missing, stop and
report — do not invent a key or guess the defaults.

**Principle amendment (authorized by Joe, 2026-07-11):** principle 3's "no
telemetry" becomes "no undisclosed, no content-bearing telemetry." What ships:
PostHog events that are **names and counts only** — never session titles,
prompts, commands, transcripts, file paths, repo names, or hostnames. A
Settings toggle controls it; the event schema below is the complete,
documented list. PLAN.md principle 3 must be updated to say exactly this.

## Inputs (Joe fills these in — the builder blocks without them)

- `POSTHOG_PROJECT_KEY`: phc_…            ← from posthog.com project settings
- `POSTHOG_HOST`: https://us.i.posthog.com | https://eu.i.posthog.com
- App analytics default: ON (disclosed at first launch) | OFF (opt-in)
- Site pageviews on hypervisor.sh: yes | no

## Architecture — capture from Rust, not the webview

The main window's CSP allows zero remote content; keep it that way. Events
are sent from the **Rust backend** (ureq is already a dependency; PostHog's
ingest API is a plain `POST {host}/capture` with `{api_key, event,
distinct_id, properties}` — no SDK needed):

- `src-tauri/src/telemetry.rs`: a bounded in-memory queue + background
  flusher (batch endpoint, ≤ every 30s), fire-and-forget — a PostHog outage
  must never block or crash anything (send failures are dropped silently
  after one retry).
- `distinct_id`: a random UUID generated once, persisted in settings.json.
  No hardware ids, no username, no hostname.
- Gate: `settings.analytics` checked before enqueue. Toggle lives in the
  Settings view ("analytics — anonymous feature counts, never content";
  the row links to this schema). If default is ON, first launch shows a
  one-time toast: "anonymous usage analytics are on — Settings to turn off."
- **Key embedding (Joe's distribution decision, 2026-07-11):** the official
  `phc_` key is baked into the builds people download — the release script
  and CI inject `POSTHOG_PROJECT_KEY` + `POSTHOG_HOST` from GitHub secrets
  (`option_env!` at compile time), so hypervisor.sh downloads report to our
  project by default (disclosed + toggleable, as above). Anyone building
  from source gets it "for themselves otherwise": no env = telemetry fully
  inert; set your own key/host env = your own PostHog project. Document
  this in the README's build section.

## Event schema (complete list — adding an event = amending this file)

| event | properties (all content-free) |
|---|---|
| `app_opened` | version, harness_counts {claude,codex,cursor,opencode: n} |
| `session_spawned` | harness, via ("new"/"subagents") |
| `session_adopted` | harness |
| `approval_resolved` | via ("tab"/"yolo"/"remote"/"notification"), decision ("approve"/"deny") |
| `prompt_sent` | tier ("tmux"/"api"), via ("desktop"/"remote"/"imessage") |
| `command_used` | name ("/rename", "/broadcast", …) — the name only, never arguments |
| `tv_toggled` | on (bool) |
| `session_archived` | bulk (bool) |
| `remote_page_opened` | (no properties) |

Explicitly forbidden in properties: any free string from user or session
data. `cargo test` gains a test asserting every capture call site uses the
typed event enum (no raw string properties possible by construction).

## Site (only if Joe's input says yes)

`posthog-js` on hypervisor.sh via npm bundle into the static page (no CDN
script tag), pageview + download-click only, `persistence: 'memory'` (no
cookies — no banner needed), respect Do Not Track. Amend DEPLOY's site spec
accordingly (its "no analytics" line becomes "PostHog per tasks/POSTHOG.md").

## Definition of done

1. Toggle off in Settings → zero requests to the PostHog host (prove with a
   proxy/log or by pointing at a mock server).
2. Toggle on → events arrive in the PostHog project (screenshot/insight
   listing the schema events after a manual test run of each action).
3. Grep-proof: no session title/prompt/cwd string reaches telemetry.rs
   (the typed-enum test + a code-review note in Evidence).
4. Offline/blocked PostHog host → app behaves identically (verified).
5. settings.json persists the toggle + distinct_id across restarts.
6. `python3 spike/compare.py --limit 20` OK · tsc · `cargo test --lib` ·
   tauri dev boots.
7. PLAN.md principle 3 amended with the exact language above.

## Scope fence

- Rust-side capture only; main-window CSP untouched.
- No session replay, no autocapture, no user identification beyond the
  random UUID, no server-side flags. Counts only.
- iMessage/remote surfaces report `via` labels through existing command
  paths — no telemetry code in the phone page itself.

## When done

Evidence per DoD, tick POSTHOG in PLAN.md, commit:
`POSTHOG: content-free analytics — typed events, settings gate, rust-side capture`.

## Evidence

(builder fills this in)
