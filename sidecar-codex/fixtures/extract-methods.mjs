#!/usr/bin/env node
// Writes the client→server method list of a codex app-server, for
// test-protocol-methods.mjs (the whole schema is megabytes; the list is enough).
//
//   node extract-methods.mjs <schema dir> <label> > codex-<label>.methods.json
//     the dir `codex app-server generate-json-schema --experimental --out <dir>` wrote
//   node extract-methods.mjs <codex-rs/app-server-protocol/src/protocol/common.rs> <label>
//     a source checkout, for a codex that is not installed (it is never built here)
import { readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const [src, label] = process.argv.slice(2)
if (!src || !label) {
  console.error('usage: extract-methods.mjs <schema dir | common.rs> <label>')
  process.exit(2)
}

function fromSchema(file) {
  const methods = new Set()
  const walk = (o) => {
    if (!o || typeof o !== 'object') return
    const m = o.properties?.method?.enum
    if (Array.isArray(m)) for (const name of m) methods.add(name)
    for (const v of Object.values(o)) walk(v)
  }
  walk(JSON.parse(readFileSync(file, 'utf8')))
  return [...methods].sort()
}

// The `<macro>! { … }` block's `Name => "method"` entries; a bare `Name,`
// notification is named in camelCase.
function fromSource(text, macro) {
  const start = text.indexOf(`\n${macro}! {`)
  if (start < 0) throw new Error(`${macro} not found`)
  const body = text.slice(start, text.indexOf('\n}', start))
  const methods = [...body.matchAll(/^\s+[A-Za-z0-9]+ => "([^"]+)"/gm)].map((m) => m[1])
  for (const m of body.matchAll(/^\s+([A-Z][A-Za-z0-9]*),$/gm)) methods.push(m[1][0].toLowerCase() + m[1].slice(1))
  return [...new Set(methods)].sort()
}

let clientRequests
let clientNotifications
if (statSync(src).isDirectory()) {
  clientRequests = fromSchema(join(src, 'ClientRequest.json'))
  clientNotifications = fromSchema(join(src, 'ClientNotification.json'))
} else {
  const text = readFileSync(src, 'utf8')
  clientRequests = fromSource(text, 'client_request_definitions')
  clientNotifications = fromSource(text, 'client_notification_definitions')
}
process.stdout.write(JSON.stringify({ label, clientRequests, clientNotifications }, null, 2) + '\n')
