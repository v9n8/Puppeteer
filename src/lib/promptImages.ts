// Pure helpers for the images attached to a sent prompt (RT-24): the data URL
// a base64 attachment is shown with, and the ‹ / › step of the viewer.
import type { ImageAttachment } from './streamEvents'

/** The `data:` URL of a base64 image attachment. */
export function imageDataUrl(img: ImageAttachment): string {
  return `data:${img.media_type};base64,${img.data}`
}

/** The index `delta` steps from `index` among `count` images, wrapping round. */
export function stepImageIndex(index: number, delta: number, count: number): number {
  if (count < 1) return 0
  return (((index + delta) % count) + count) % count
}
