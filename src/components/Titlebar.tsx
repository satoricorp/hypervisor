import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";

export function Titlebar() {
  const { dispatch } = useStore();
  const [tvOn, setTvOn] = useState(false);

  // TV design-review prototype (design/tv.md). Plain click toggles the PiP
  // player; ⌥-click simulates a needs-you interrupt so the pause/strip flow
  // can be reviewed without waiting for a real stalled session.
  async function onTv(simulate: boolean) {
    try {
      if (simulate) {
        if (!tvOn) {
          setTvOn(await invoke<boolean>("toggle_tv"));
          await new Promise((r) => setTimeout(r, 3000)); // let youtube load
        }
        await invoke("tv_interrupt", {
          title: "5 · integration tests for billing webhooks",
          detail: "wants to run Bash(stripe fixtures pull --api-version 2026-03)",
        });
      } else {
        setTvOn(await invoke<boolean>("toggle_tv"));
      }
    } catch (e) {
      dispatch({ type: "TOAST", html: `tv: ${String(e)}` });
    }
  }

  return (
    <div className="titlebar">
      <span className="mark">
        HYPERVISOR<small>v0.1 · variant B</small>
      </span>
      <button
        className={"tvbtn" + (tvOn ? " on" : "")}
        id="tvbtn"
        title="picture-in-picture tv — ⌥-click simulates an interrupt"
        type="button"
        onClick={(e) => onTv(e.altKey)}
      >
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="2.5" y="4.5" width="19" height="14" rx="2" />
          <rect x="11.5" y="11" width="7" height="4.8" rx="1" fill="currentColor" stroke="none" />
        </svg>
        tv
      </button>
      <button
        className="menukey"
        id="menukey"
        title="command palette"
        type="button"
        onClick={() => dispatch({ type: "OPEN_PALETTE" })}
      >
        ⌘K
      </button>
    </div>
  );
}
