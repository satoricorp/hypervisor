import { useEffect, useState } from "react";
import { CTL_HINT, STATE_META, buildLog, iconOf } from "../constants";
import { HISTORY } from "../mockSessions";
import { useStore } from "../store";
import { SubagentList } from "./SubagentList";
import { SubTranscript, Transcript } from "./Transcript";

function UsagePane() {
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Usage</h4>
      <div className="tiles">
        <div className="tile">
          <div className="lbl">API spend · today</div>
          <div className="val">$4.51</div>
          <div className="sub2">week $23.80</div>
        </div>
        <div className="tile">
          <div className="lbl">API tokens</div>
          <div className="val">2.41M</div>
          <div className="sub2">in 1.9M · out 0.5M</div>
        </div>
        <div className="tile">
          <div className="lbl">Subscription tokens</div>
          <div className="val">3.04M</div>
          <div className="sub2">$0 marginal</div>
        </div>
        <div className="tile">
          <div className="lbl">Handoffs</div>
          <div className="val">3</div>
          <div className="sub2">opencode ×2 · codex ×1</div>
        </div>
      </div>
      <h4>Cost by model — API · today</h4>
      <div className="bars">
        <div className="barrow">
          <div className="name">
            claude-fable-5 <span>· anthropic</span>
          </div>
          <div className="track">
            <div className="fill" style={{ width: "100%" }} />
          </div>
          <div className="num">
            $2.04 <span>· 812K tok</span>
          </div>
        </div>
        <div className="barrow">
          <div className="name">
            gpt-5 <span>· openai</span>
          </div>
          <div className="track">
            <div className="fill" style={{ width: "58%" }} />
          </div>
          <div className="num">
            $1.18 <span>· 640K tok</span>
          </div>
        </div>
        <div className="barrow">
          <div className="name">
            claude-sonnet-5 <span>· anthropic</span>
          </div>
          <div className="track">
            <div className="fill" style={{ width: "44%" }} />
          </div>
          <div className="num">
            $0.89 <span>· 594K tok</span>
          </div>
        </div>
        <div className="barrow">
          <div className="name">
            o4-mini <span>· openai</span>
          </div>
          <div className="track">
            <div className="fill" style={{ width: "15%" }} />
          </div>
          <div className="num">
            $0.31 <span>· 238K tok</span>
          </div>
        </div>
        <div className="barrow">
          <div className="name">
            glm-5.2 <span>· opencode</span>
          </div>
          <div className="track">
            <div className="fill" style={{ width: "5%" }} />
          </div>
          <div className="num">
            $0.09 <span>· 126K tok</span>
          </div>
        </div>
      </div>
      <h4>Included in subscriptions</h4>
      <div className="listrow">
        <span>claude code</span>
        <span className="dim">· claude max 20×</span>
        <span className="tabnum">1.90M tok</span>
        <span className="chip-inc">included</span>
      </div>
      <div className="listrow">
        <span>codex</span>
        <span className="dim">· chatgpt pro</span>
        <span className="tabnum">0.73M tok</span>
        <span className="chip-inc">included</span>
      </div>
      <div className="listrow">
        <span>cursor</span>
        <span className="dim">· cursor pro</span>
        <span className="tabnum">0.41M tok</span>
        <span className="chip-inc">included</span>
      </div>
      <p className="footnote">
        costs approximate — token counts from session logs; pricing synced jul
        2026.
      </p>
    </div>
  );
}

function AccessPane() {
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Keys &amp; subscriptions</h4>
      <div className="listrow">
        <span className="grow">ANTHROPIC_API_KEY</span>
        <span className="dim">env · ~/.zshrc</span>
        <span className="dim">sk-ant-…4Q2A</span>
        <span className="tabnum st-done">● active</span>
      </div>
      <div className="listrow">
        <span className="grow">OPENAI_API_KEY</span>
        <span className="dim">keychain</span>
        <span className="dim">sk-proj-…9fKM</span>
        <span className="tabnum st-done">● active</span>
      </div>
      <div className="listrow">
        <span className="grow">claude max 20×</span>
        <span className="dim">subscription</span>
        <span className="dim">renews aug 3</span>
        <span className="tabnum st-done">● active</span>
      </div>
      <div className="listrow">
        <span className="grow">chatgpt pro</span>
        <span className="dim">subscription</span>
        <span className="dim">renews jul 22</span>
        <span className="tabnum st-done">● active</span>
      </div>
      <div className="listrow">
        <span className="grow">cursor pro</span>
        <span className="dim">subscription</span>
        <span className="dim">renews jul 30</span>
        <span className="tabnum st-done">● active</span>
      </div>
      <div className="listrow">
        <span className="grow dim">OPENROUTER_API_KEY</span>
        <span className="dim">env</span>
        <span className="dim">not found</span>
        <span className="tabnum dim">○ missing</span>
      </div>
      <p className="footnote">
        read-only — hypervisor never stores key material, never proxies or
        resells tokens: your keys, your subscriptions, zero markup.
      </p>
    </div>
  );
}

function Switch({ initialOn = false }: { initialOn?: boolean }) {
  const [on, setOn] = useState(initialOn);
  return (
    <button
      type="button"
      className={`switch ${on ? "on" : ""}`}
      onClick={() => setOn((v) => !v)}
    >
      <i />
    </button>
  );
}

function SettingsPane() {
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>Notifications</h4>
      <div className="listrow">
        <span>notify when a session responds</span>
        <span className="dim">notification center</span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>play sound on done</span>
        <span className="dim">ping</span>
        <Switch initialOn />
      </div>
      <h4>Sources</h4>
      <div className="listrow">
        <span>claude code</span>
        <span className="dim">hooks + transcripts</span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>codex</span>
        <span className="dim">session files</span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>opencode</span>
        <span className="dim">http api</span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>cursor</span>
        <span className="dim">state.vscdb</span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>claude.ai</span>
        <span className="dim">browser extension — not installed</span>
        <Switch />
      </div>
      <h4>General</h4>
      <div className="listrow">
        <span>auto-worktree when a repo is busy</span>
        <span className="dim">
          new session in an occupied repo gets its own worktree
        </span>
        <Switch initialOn />
      </div>
      <div className="listrow">
        <span>launch at login</span>
        <Switch initialOn />
      </div>
      <p className="footnote">mocked — toggles flip but persist nothing.</p>
    </div>
  );
}

function HistoryPane() {
  const { state, dispatch } = useStore();
  const q = state.historyFilter.toLowerCase();
  const rows = HISTORY.filter(
    (h) =>
      !q ||
      [h.title, h.note, h.model, h.app].join(" ").toLowerCase().includes(q),
  );
  return (
    <div className="pane">
      <span className="escnote">esc ↩ session</span>
      <h4>History</h4>
      <input
        className="hq"
        id="hq"
        type="text"
        placeholder="search finished sessions…"
        spellCheck={false}
        autoComplete="off"
        value={state.historyFilter}
        onChange={(e) =>
          dispatch({ type: "SET_HISTORY_FILTER", value: e.target.value })
        }
      />
      <div id="hrows">
        {rows.map((h, i) => (
          <div key={i} className="listrow">
            <span className="dim" style={{ width: 88, flex: "none" }}>
              {h.when}
            </span>
            <span className="grow">{h.title}</span>
            <span className="dim grow">{h.note}</span>
            <span className="tabnum">{h.num}</span>
            <span className="modelchip">{h.model}</span>
            <span
              className="hicon"
              title={h.app}
              dangerouslySetInnerHTML={{ __html: iconOf(h.app) }}
            />
          </div>
        ))}
      </div>
      <p className="footnote">
        stored locally — sqlite at ~/Library/Application
        Support/Hypervisor/history.db · export as jsonl
      </p>
    </div>
  );
}

function SessionView() {
  const { state } = useStore();
  const s = state.sessions[state.sel];

  useEffect(() => {
    const el = document.getElementById("dlog");
    if (el) el.scrollTop = el.scrollHeight;
  }, [state.sel, state.subSel, s.state, s.log, s.thinkIdx]);

  if (state.subSel >= 0 && s.subs[state.subSel]) {
    const x = s.subs[state.subSel];
    return (
      <div className="dwrap">
        <div className="dhead">
          <span className={`status ${STATE_META[x.state].cls}`}>
            <i className="dot" />
          </span>
          <h3>{x.task || "subagent"}</h3>
          <span className="meta">
            <span className="modelchip">{x.model}</span>
            <span
              className="hicon"
              title={x.target}
              dangerouslySetInnerHTML={{ __html: iconOf(x.target) }}
            />
          </span>
        </div>
        <p className="dfrom">
          ↳ subagent {state.sel + 1}·{state.subSel + 1} of “{s.title}” · h
          steps back up
        </p>
        <SubTranscript sub={x} />
      </div>
    );
  }

  const m = STATE_META[s.state];
  const hint = CTL_HINT[s.ctl] || CTL_HINT.tmux;
  const log = s.log ?? buildLog(s);

  return (
    <div className="dwrap">
      <div className="dhead">
        <span className={`status ${m.cls}`}>
          <i className="dot" />
        </span>
        <h3>{s.title}</h3>
        <span className="meta">
          {s.loop ? <span className="loopchip">↻ loop</span> : null}
          <span
            className={`ctlchip ${s.ctl === "observe" ? "observe" : ""}`}
            title={hint.tip}
          >
            {hint.label}
          </span>
          <span className="modelchip">{s.model}</span>
          <span
            className="hicon"
            title={s.app}
            dangerouslySetInnerHTML={{ __html: iconOf(s.app) }}
          />
        </span>
      </div>
      {s.repo ? (
        <p className="dfrom">
          ⎇ {s.repo} · {s.br || "main"}
          {s.wt ? ` · worktree ${s.wt}` : ""}
        </p>
      ) : null}
      <SubagentList />
      <Transcript session={s} log={log} />
    </div>
  );
}

export function MainPane() {
  const { state } = useStore();
  let body;
  if (state.view === "usage") body = <UsagePane />;
  else if (state.view === "access") body = <AccessPane />;
  else if (state.view === "settings") body = <SettingsPane />;
  else if (state.view === "history") body = <HistoryPane />;
  else body = <SessionView />;
  return <main id="main">{body}</main>;
}
