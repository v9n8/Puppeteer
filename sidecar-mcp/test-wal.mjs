#!/usr/bin/env node
// The built-in `puppeteer` MCP server on a data.db in WAL mode, which is how the app opens it
// (src-tauri/src/db.rs). The journal mode is stored in the file, so the sidecar never sets it:
// it must read while the app holds an open write transaction, and wait out (not fail on) the
// app's write lock when it writes.
//
// Builds a throwaway DB from every real migration, switches it to WAL, holds transactions on
// one connection (standing in for the app) and drives `index.mjs` over stdio. No network.
//
// Usage: node sidecar-mcp/test-wal.mjs

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const SERVER = join(HERE, 'index.mjs');
const MIGRATIONS = join(HERE, '..', 'src-tauri', 'migrations');
const PROJECT = 'proj-1';
const T0 = '2026-01-01T00:00:00Z';

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  process.stdout.write(`${ok ? '  ok  ' : '  FAIL'} ${label}\n`);
};

// ---- Fixture -------------------------------------------------------------------

const appData = mkdtempSync(join(tmpdir(), 'puppeteer-wal-'));
const dbPath = join(appData, 'data.db');
const app = new DatabaseSync(dbPath);
app.exec('PRAGMA journal_mode = WAL');
for (const f of readdirSync(MIGRATIONS).filter((f) => /^\d{4}_.*\.sql$/.test(f)).sort()) {
  app.exec(readFileSync(join(MIGRATIONS, f), 'utf8'));
}
app.prepare("INSERT INTO projects (id, name, path, created_at) VALUES (?, 'proj', '/tmp/proj-1', ?)").run(PROJECT, T0);
app.prepare("INSERT INTO todos (id, text, done, position, created_at, project_id) VALUES ('t-1', 'committed', 0, 0, ?, ?)")
  .run(T0, PROJECT);
check(app.prepare('PRAGMA journal_mode').get().journal_mode === 'wal', 'fixture: data.db is in WAL mode');
check(existsSync(`${dbPath}-wal`) && existsSync(`${dbPath}-shm`), 'fixture: -wal and -shm sit beside data.db');

// ---- Drive the server -------------------------------------------------------------

function drive(calls) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [SERVER], {
      env: { ...process.env, PUPPETEER_DB_PATH: dbPath, PUPPETEER_PROJECT_ID: PROJECT },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let out = '';
    child.stdout.on('data', (d) => (out += d));
    child.on('error', reject);
    child.on('close', () => {
      const byId = new Map();
      for (const line of out.split('\n').filter((l) => l.trim())) {
        try {
          const m = JSON.parse(line);
          byId.set(m.id, m);
        } catch {
          /* not a frame */
        }
      }
      resolve(Object.fromEntries(calls.map(([label], i) => [label, byId.get(i + 1)?.result?.content?.[0]?.text ?? ''])));
    });
    calls.forEach(([, name, args], i) =>
      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: i + 1, method: 'tools/call', params: { name, arguments: args } })}\n`),
    );
    child.stdin.end();
  });
}

// A read while the app holds an open write transaction: under the old rollback journal an
// EXCLUSIVE transaction shut every reader out; under WAL the reader sees the last commit.
app.exec('BEGIN EXCLUSIVE');
app.prepare("INSERT INTO todos (id, text, done, position, created_at, project_id) VALUES ('t-2', 'uncommitted', 0, 1, ?, ?)")
  .run(T0, PROJECT);
let started = Date.now();
const read = await drive([['list', 'todos_list', {}]]);
const readMs = Date.now() - started;
app.exec('COMMIT');
check(read.list.includes('committed') && !read.list.includes('uncommitted'), `read during an app write: last commit seen (got: ${read.list})`);
check(readMs < 5000, `read during an app write: not kept waiting (${readMs} ms)`);

// A write while the app holds the write lock: the sidecar waits for it (busy_timeout), then commits.
app.exec('BEGIN IMMEDIATE');
const release = setTimeout(() => app.exec('COMMIT'), 300);
started = Date.now();
const write = await drive([['add', 'todos_add', { text: 'from the sidecar' }]]);
clearTimeout(release);
if (app.isTransaction) app.exec('COMMIT');
const added = app.prepare("SELECT COUNT(*) AS n FROM todos WHERE text = 'from the sidecar'").get().n;
check(added === 1, `write behind the app's lock: waited, then committed (got: ${write.add}, ${Date.now() - started} ms)`);
check(app.prepare('PRAGMA journal_mode').get().journal_mode === 'wal', 'the sidecar leaves the journal mode alone');

// The sidecar's own connection: busy_timeout matches the app's (db.rs BUSY_TIMEOUT).
process.env.PUPPETEER_DB_PATH = dbPath;
const { db: storeDb } = await import('./lib/store.mjs');
check(Object.values(storeDb().prepare('PRAGMA busy_timeout').get())[0] === 10000, 'store: busy_timeout is 10 s');
storeDb().close();

app.close();
rmSync(appData, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
