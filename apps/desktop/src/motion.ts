import { cubicOut } from "svelte/easing";

export const POPOVER_DURATION_MS = 220;

function prefersReducedMotion() {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** Shared dropdown transition: a short spatial settle with a quiet fade. */
export function popover(_node: Element) {
  return {
    duration: prefersReducedMotion() ? 0 : POPOVER_DURATION_MS,
    easing: cubicOut,
    css: (progress: number) => {
      const inverse = 1 - progress;
      return `opacity:${progress};transform:translate3d(0,${-8 * inverse}px,0) scale(${0.95 + progress * 0.05})`;
    },
  };
}
