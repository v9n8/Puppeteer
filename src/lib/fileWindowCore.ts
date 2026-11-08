// The URL query of a file pop-out (views/FileViewerWindow.vue): built by
// lib/fileWindow.ts, read back by the window. Kept free of Vue / Tauri / `@/`
// imports so it can be unit-tested with `node --test`.

export interface FileWindowParams {
  projectPath: string
  path: string
  /** Line to highlight / scroll to. */
  line: number | null
  /** The run that opened the file; its Quote goes there (null = the project's latest session). */
  originRunId: string | null
}

/** The query string (no `?`) of `index.html` for a file window. */
export function fileWindowQuery(p: FileWindowParams): string {
  const params = new URLSearchParams({ fileWindow: '1', projectPath: p.projectPath, path: p.path })
  if (p.line != null) params.set('line', String(p.line))
  if (p.originRunId) params.set('originRunId', p.originRunId)
  return params.toString()
}

/** Read a file window's `location.search` back. */
export function parseFileWindowQuery(search: string): FileWindowParams {
  const params = new URLSearchParams(search)
  const line = params.get('line')
  return {
    projectPath: params.get('projectPath') ?? '',
    path: params.get('path') ?? '',
    line: line != null ? Number(line) : null,
    originRunId: params.get('originRunId') || null,
  }
}
