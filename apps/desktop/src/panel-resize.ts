export function readPanelSize(key: string, fallback: number): number {
  try {
    const stored = Number(window.localStorage.getItem(key));
    return Number.isFinite(stored) && stored > 0 ? stored : fallback;
  } catch {
    return fallback;
  }
}

export function storePanelSize(key: string, value: number) {
  try {
    window.localStorage.setItem(key, String(Math.round(value)));
  } catch {
    // Resizing remains available when WebView storage is unavailable.
  }
}

export const clampPanelSize = (value: number, minimum: number, maximum: number) =>
  Math.min(maximum, Math.max(minimum, value));

export function beginHorizontalResize(
  event: PointerEvent,
  valueAt: (clientX: number) => number,
  onResize: (value: number) => void,
  onCommit: (value: number) => void,
) {
  event.preventDefault();
  const handle = event.currentTarget as HTMLElement;
  handle.setPointerCapture?.(event.pointerId);
  document.documentElement.classList.add("panel-resizing");
  let latest = valueAt(event.clientX);
  onResize(latest);

  const move = (next: PointerEvent) => {
    latest = valueAt(next.clientX);
    onResize(latest);
  };
  const finish = () => {
    handle.releasePointerCapture?.(event.pointerId);
    document.documentElement.classList.remove("panel-resizing");
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", finish);
    onCommit(latest);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", finish, { once: true });
  window.addEventListener("pointercancel", finish, { once: true });
}
