/**
 * Fake async timers for M0 mock data.
 * M2 deletes this module and wires real tauri events instead.
 */

type CompleteFn = (sessionIndex: number) => void;
type SubDoneFn = (sessionIndex: number, subIndex: number) => void;
type ThinkTickFn = () => void;
type PlanReadyFn = (sessionIndex: number) => void;

const sessionTimers = new Map<number, ReturnType<typeof setTimeout>>();
let thinkInterval: ReturnType<typeof setInterval> | null = null;

export function scheduleSessionComplete(
  i: number,
  onComplete: CompleteFn,
  ms?: number,
) {
  clearTimeout(sessionTimers.get(i));
  const delay = ms ?? 4200 + Math.random() * 2500;
  sessionTimers.set(
    i,
    setTimeout(() => {
      sessionTimers.delete(i);
      onComplete(i);
    }, delay),
  );
}

export function scheduleSubagentDone(
  onDone: SubDoneFn,
  sessionIndex: number,
  subIndex: number,
  ms?: number,
) {
  const delay = ms ?? 6000 + Math.random() * 3000;
  setTimeout(() => onDone(sessionIndex, subIndex), delay);
}

export function scheduleSubPromptDone(
  onDone: SubDoneFn,
  sessionIndex: number,
  subIndex: number,
) {
  setTimeout(
    () => onDone(sessionIndex, subIndex),
    4000 + Math.random() * 2500,
  );
}

export function schedulePlanReady(i: number, onReady: PlanReadyFn) {
  clearTimeout(sessionTimers.get(i));
  sessionTimers.set(
    i,
    setTimeout(() => {
      sessionTimers.delete(i);
      onReady(i);
    }, 4500),
  );
}

export function scheduleYoloApprovals(
  count: number,
  onTick: (i: number) => void,
) {
  for (let i = 0; i < count; i++) {
    setTimeout(() => onTick(i), 700 + i * 400);
  }
}

export function startThinkingCycler(onTick: ThinkTickFn) {
  if (thinkInterval) return;
  thinkInterval = setInterval(onTick, 3200);
}

export function stopThinkingCycler() {
  if (thinkInterval) {
    clearInterval(thinkInterval);
    thinkInterval = null;
  }
}

export function clearSessionTimer(i: number) {
  clearTimeout(sessionTimers.get(i));
  sessionTimers.delete(i);
}
