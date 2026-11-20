// One beat for every looping loading animation (RT-28). A CSS animation counts
// from the moment its element mounted (or got its class), so two Sessions that
// started running 0.3 s apart spin 0.3 s apart for ever, and a pop-out window,
// with its own timeline, drifts further. Here every such animation is started
// at the wall-clock epoch instead: its phase is then the same in every element
// and every window at any instant. `animation-delay` (the staggered "thinking"
// dots) is part of the effect's timing, so it still applies on top.
// `prefers-reduced-motion` sets `animation: none`, which fires no event.

/** Exact Tailwind / main.css names that loop while something loads. */
const SYNCED_NAMES = new Set(['spin', 'ping', 'pulse', 'bounce', 'skeleton-sweep'])
/** Prefixes: scoped `<style>` blocks rename keyframes to `<name>-<hash>`. */
const SYNCED_PREFIXES = ['thinking-']

/** Whether an animation of this name should follow the shared beat. */
export function shouldSync(name: string): boolean {
  return SYNCED_NAMES.has(name) || SYNCED_PREFIXES.some((p) => name.startsWith(p))
}

/**
 * The start time, on a document timeline that began at `timeOrigin` (epoch ms,
 * `performance.timeOrigin`), that puts an animation's local time at the epoch.
 */
export function syncedStartTime(timeOrigin: number): number {
  return -timeOrigin
}

/** Where in its cycle (ms, in [0, duration)) an animation is at `localTime`. */
export function animationPhase(localTime: number, duration: number, delay = 0): number {
  if (!(duration > 0)) return 0
  return (((localTime - delay) % duration) + duration) % duration
}

let installed = false

/** Listen (once per window) for looping loading animations and align them. */
export function installAnimationSync(): void {
  if (installed || typeof document === 'undefined') return
  installed = true
  document.addEventListener(
    'animationstart',
    (e: AnimationEvent) => {
      if (!shouldSync(e.animationName)) return
      const el = e.target as Element | null
      if (!el || typeof el.getAnimations !== 'function') return
      const start = syncedStartTime(performance.timeOrigin)
      // `subtree` so a pseudo-element's animation (`.skeleton::after`) is found too.
      for (const a of el.getAnimations({ subtree: true })) {
        if (!(a instanceof CSSAnimation) || a.animationName !== e.animationName) continue
        const fx = a.effect as KeyframeEffect | null
        if (fx?.target !== el || (fx.pseudoElement ?? '') !== e.pseudoElement) continue
        if (a.startTime !== start) a.startTime = start
      }
    },
    true,
  )
}
