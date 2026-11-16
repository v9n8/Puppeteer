import { onBeforeUnmount, ref, type Ref } from 'vue'

/** A `Date` ref that refreshes at the start of every minute; stops on unmount. */
export function useMinuteClock(): Ref<Date> {
  const now = ref(new Date())
  let timer: ReturnType<typeof setTimeout> | null = null
  const schedule = () => {
    const wait = 60_000 - (Date.now() % 60_000) + 50
    timer = setTimeout(() => {
      now.value = new Date()
      schedule()
    }, wait)
  }
  schedule()
  onBeforeUnmount(() => { if (timer) clearTimeout(timer) })
  return now
}
