// "Compare with…" state of a file viewer: the second-file picker, and — for text
// files — the second file the diff is showing. Images go to the app's side-by-side
// compare host (stores/imageCompare) instead.
import { computed, ref, watch, type Ref } from 'vue'
import { compareKind } from '@/lib/markdownOutline'
import { useImageCompareStore } from '@/stores/imageCompare'

// "Compare with…" chosen in a file tab's right-click menu (SessionTabBar), which
// has no handle on that tab's viewer: the viewer opened on that file takes it up
// once loaded (the picker needs the file's kind).
const compareRequest = ref<{ projectPath: string; path: string } | null>(null)

export function requestCompare(projectPath: string, path: string) {
  compareRequest.value = { projectPath, path }
}

/** `opened`: the path the viewer was asked for (the loaded one may be normalized). */
export function useFileCompare(
  projectPath: Ref<string>, path: Ref<string>, editing: Ref<boolean>, opened: () => string, loading: Ref<boolean>,
) {
  const imageCompare = useImageCompareStore()
  const kind = computed(() => compareKind(path.value))
  const pickerOpen = ref(false)
  /** Relative path of the file the current text diff is against; '' when not comparing. */
  const second = ref('')

  function openPicker() {
    if (kind.value) pickerOpen.value = true
  }

  function pick(other: string) {
    pickerOpen.value = false
    if (kind.value === 'image') {
      imageCompare.selectFirst(projectPath.value, path.value)
      imageCompare.selectSecond(other)
    } else {
      second.value = other
    }
  }

  function closeCompare() {
    second.value = ''
  }

  function reset() {
    pickerOpen.value = false
    second.value = ''
  }

  // Another file, or an edit, ends the comparison.
  watch([projectPath, path], reset)
  watch(editing, (on) => { if (on) second.value = '' })
  // Post flush: after the reset above, which a newly loaded path also triggers.
  watch([compareRequest, path, loading], () => {
    const want = compareRequest.value
    if (!want || !path.value || loading.value) return
    if (want.projectPath !== projectPath.value || want.path !== opened()) return
    compareRequest.value = null
    openPicker()
  }, { flush: 'post', immediate: true })

  return { kind, pickerOpen, second, openPicker, pick, closeCompare, reset }
}
