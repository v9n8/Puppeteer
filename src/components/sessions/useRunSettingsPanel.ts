// The Run options panel is teleported to <body> and positioned `fixed` next to
// the ⚙ button: the Session column and split view are `overflow-hidden`, and a
// narrow column cut the old `absolute` panel off whatever its z-index.
import { onBeforeUnmount, ref, watch, type Ref } from 'vue'
import { placeRunSettingsPanel } from '@/lib/runSettingsPlacement'

/** A mousedown that closes Run options: not on the ⚙ root, the teleported panel, or an AppSelect list. */
export function clickLeavesRunSettings(target: EventTarget | null, root: HTMLElement | null): boolean {
  if (!(target instanceof Element)) return false
  if (root?.contains(target)) return false
  return !target.closest('[data-run-settings-panel], [data-app-select-dropdown]')
}

export function useRunSettingsPanel(anchor: Ref<HTMLElement | null>) {
  const panel = ref<HTMLElement | null>(null)
  const pos = ref({ left: 0, top: 0 })

  function place() {
    const a = anchor.value?.getBoundingClientRect()
    const el = panel.value
    if (!a || !el) return
    pos.value = placeRunSettingsPanel(
      a,
      { width: el.offsetWidth, height: el.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    )
  }

  // Keyed on the panel element, which exists exactly while the panel is open.
  // The observer catches content that changes its height after opening (the
  // MCP list loading, Effort / Response style folding in), so it keeps
  // growing upward instead of over the button.
  let observer: ResizeObserver | null = null
  function unbind() {
    observer?.disconnect()
    observer = null
    window.removeEventListener('resize', place)
  }
  watch(panel, (el) => {
    unbind()
    if (!el) return
    place()
    window.addEventListener('resize', place)
    if (typeof ResizeObserver === 'undefined') return
    observer = new ResizeObserver(() => place())
    observer.observe(el)
  }, { flush: 'post' })
  onBeforeUnmount(unbind)

  return { panel, pos }
}
