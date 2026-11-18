/**
 * Number for a PageHeader count pill: null (pill hidden) until the list loaded
 * and while the last load failed, so a "0" never stands for "not loaded yet".
 */
export function pageCount(loaded: boolean, error: string | null, n: number): number | null {
  return loaded && !error ? n : null
}
