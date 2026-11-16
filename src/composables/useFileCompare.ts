// "Compare with…" state of a file viewer: the second-file picker, and — for text
// files — the second file the diff is showing. Images go to the app's side-by-side
// compare host (stores/imageCompare) instead.
import { computed, ref, watch, type Ref } from 'vue'
import { compareKind } from '@/lib/markdownOutline'
import { useImageCompareStore } from '@/stores/imageCompare'

export function useFileCompare(projectPath: Ref<string>, path: Ref<string>, editing: Ref<boolean>) {
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

  return { kind, pickerOpen, second, openPicker, pick, closeCompare, reset }
}
