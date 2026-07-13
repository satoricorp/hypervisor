import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";

export function Titlebar() {
  const { dispatch } = useStore();
  const [tvOn, setTvOn] = useState(false);

  // ⌘T toggles the tv from anywhere in the app (Titlebar is always mounted)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "t") {
        e.preventDefault();
        invoke<boolean>("toggle_tv")
          .then(setTvOn)
          .catch((err) =>
            dispatch({ type: "TOAST", label: "tv", detail: String(err) }),
          );
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [dispatch]);

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
      dispatch({ type: "TOAST", label: "tv", detail: String(e) });
    }
  }

  return (
    // data-tauri-drag-region makes the custom titlebar draggable (the window
    // uses titleBarStyle: Overlay, so without this the window won't move).
    <div className="titlebar" data-tauri-drag-region>
      <span className="mark" data-tauri-drag-region>
        HYPERVISOR<small>v0.1 · variant B</small>
      </span>
      <button
        className={"tvbtn" + (tvOn ? " on" : "")}
        id="tvbtn"
        title="picture-in-picture tv (⌘T) — ⌥-click simulates an interrupt"
        type="button"
        onClick={(e) => onTv(e.altKey)}
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            fill="#ffffff"
            d="M23.5 6.5a3 3 0 0 0-2.11-2.12C19.5 3.86 12 3.86 12 3.86s-7.5 0-9.39.52A3 3 0 0 0 .5 6.5 31.3 31.3 0 0 0 0 12a31.3 31.3 0 0 0 .5 5.5 3 3 0 0 0 2.11 2.12c1.89.52 9.39.52 9.39.52s7.5 0 9.39-.52A3 3 0 0 0 23.5 17.5 31.3 31.3 0 0 0 24 12a31.3 31.3 0 0 0-.5-5.5zM9.55 15.57V8.43L15.82 12l-6.27 3.57z"
          />
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
