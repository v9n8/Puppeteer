import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

/** True while `<html>` carries the `dark` class (App.vue / themeMode.ts toggle it); follows changes. */
export function useIsDark(): Ref<boolean> {
  const root = document.documentElement
  const isDark = ref(root.classList.contains('dark'))
  let observer: MutationObserver | null = null
  onMounted(() => {
    observer = new MutationObserver(() => { isDark.value = root.classList.contains('dark') })
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
  })
  onBeforeUnmount(() => observer?.disconnect())
  return isDark
}
