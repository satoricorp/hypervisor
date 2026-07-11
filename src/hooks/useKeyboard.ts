import { useEffect } from "react";
import { toggleTv } from "../api";
import { chooseMenu, doApprove, doArchive, doSend, useStore } from "../store";

/**
 * Global keydown — order matches design/mockup-b.html exactly:
 * ⌘K → palette branch → other-input guard → esc → menu → prompt →
 * Tab → digits → j/k → h/l → ⏎ → / → any-letter-focuses-prompt.
 * Plus ⌘N for New Agent (Tauri; browsers reserve it).
 * Plus ⌘⌫ to archive the selected session (ARCHIVE).
 */
export function useKeyboard() {
  const { state, dispatch, promptRef } = useStore();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (state.palette.open) dispatch({ type: "CLOSE_PALETTE" });
        else dispatch({ type: "OPEN_PALETTE" });
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        dispatch({ type: "START_NEW_AGENT" });
        requestAnimationFrame(() => promptRef.current?.focus());
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "Backspace") {
        e.preventDefault();
        void doArchive(state, dispatch);
        return;
      }
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      if (state.palette.open) {
        if (e.key === "Escape") dispatch({ type: "CLOSE_PALETTE" });
        else if (e.key === "ArrowDown" || e.key === "j") {
          e.preventDefault();
          dispatch({
            type: "PAL_ACTIVE",
            active: Math.min(
              state.palette.active + 1,
              state.palette.items.length - 1,
            ),
          });
        } else if (e.key === "ArrowUp" || e.key === "k") {
          e.preventDefault();
          dispatch({
            type: "PAL_ACTIVE",
            active: Math.max(state.palette.active - 1, 0),
          });
        } else if (e.key === "Enter") {
          e.preventDefault();
          dispatch({ type: "CHOOSE_PAL" });
          const it = state.palette.items[state.palette.active];
          if (it?.id === "subagents") {
            requestAnimationFrame(() => promptRef.current?.focus());
          }
          if (it?.id === "tv") {
            toggleTv().catch((err) =>
              dispatch({ type: "TOAST", label: "tv", detail: String(err) }),
            );
          }
        }
        return;
      }

      const ae = document.activeElement as HTMLElement | null;
      const input = promptRef.current;
      if (
        ae !== input &&
        ae &&
        (ae.tagName === "INPUT" || ae.tagName === "TEXTAREA")
      ) {
        if (e.key === "Escape") ae.blur();
        return;
      }
      const inInput = ae === input;

      if (e.key === "Escape") {
        if (state.menu.open) dispatch({ type: "MENU_STEP_BACK" });
        else if (inInput) input?.blur();
        else if (state.view !== "session")
          dispatch({ type: "SHOW_VIEW", view: "session" });
        return;
      }

      if (state.menu.open && inInput) {
        const jk = state.menu.step !== "root";
        if (e.key === "ArrowDown" || (jk && e.key === "j")) {
          e.preventDefault();
          dispatch({
            type: "MENU_ACTIVE",
            active: Math.min(
              state.menu.active + 1,
              state.menu.items.length - 1,
            ),
          });
          return;
        }
        if (e.key === "ArrowUp" || (jk && e.key === "k")) {
          e.preventDefault();
          dispatch({
            type: "MENU_ACTIVE",
            active: Math.max(state.menu.active - 1, 0),
          });
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault();
          chooseMenu(state, dispatch);
          return;
        }
      }
      if (inInput) {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          void doSend(state, dispatch);
        }
        // Shift+Enter → newline (textarea default)
        return;
      }

      if (e.key === "Tab") {
        e.preventDefault();
        void doApprove(state, dispatch);
        return;
      }
      if (/^[1-9]$/.test(e.key)) {
        const want = +e.key;
        const idx = state.sessions.findIndex((s) => s.n === want);
        if (idx >= 0) dispatch({ type: "SELECT", i: idx });
        return;
      }
      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        dispatch({ type: "SELECT", i: state.sel + 1 });
        return;
      }
      if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        dispatch({ type: "SELECT", i: state.sel - 1 });
        return;
      }
      if (e.key === "ArrowRight" || e.key === "l") {
        e.preventDefault();
        dispatch({ type: "MOVE_SUB", dir: 1 });
        return;
      }
      if (e.key === "ArrowLeft" || e.key === "h") {
        e.preventDefault();
        dispatch({ type: "MOVE_SUB", dir: -1 });
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        const s = state.sessions[state.sel];
        if (s && state.subSel < 0 && s.ctl === "observe") {
          void doSend(state, dispatch);
        } else {
          input?.focus();
        }
        return;
      }
      if (e.key === "/") {
        e.preventDefault();
        input?.focus();
        dispatch({ type: "SET_PROMPT", value: "/" });
        dispatch({ type: "OPEN_MENU" });
        return;
      }
      if (e.key.length === 1 && /\S/.test(e.key)) input?.focus();
    };

    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [state, dispatch, promptRef]);
}
