#!/usr/bin/env node
// Skill tools of the built-in `puppeteer` MCP server on the per-engine `project_skills`
// schema (migration 0002): a target change leaves the projects that have the skill alone,
// and a delete drops its project rows and the links projects had to it.
//
// Builds a throwaway DB from every real migration and a project with the skill linked for
// both engines, then drives `index.mjs` over stdio. No network.
//
// Usage: node sidecar-mcp/test-skills.mjs

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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

const appData = realpathSync(mkdtempSync(join(tmpdir(), 'puppeteer-skills-')));
const dbPath = join(appData, 'data.db');
const db = new DatabaseSync(dbPath);
for (const f of readdirSync(MIGRATIONS).filter((f) => /^\d{4}_.*\.sql$/.test(f)).sort()) {
  db.exec(readFileSync(join(MIGRATIONS, f), 'utf8'));
}
const source = join(appData, 'skills', 'review');
mkdirSync(source, { recursive: true });
writeFileSync(join(source, 'SKILL.md'), '---\nname: review\n---\nReview it.\n');
const project = join(appData, 'proj');
const links = ['.claude', '.agents'].map((engine) => join(project, engine, 'skills', 'review'));
for (const link of links) {
  mkdirSync(dirname(link), { recursive: true });
  symlinkSync(source, link);
}
db.prepare("INSERT INTO projects (id, name, path, created_at) VALUES (?, 'proj', ?, ?)").run(PROJECT, project, T0);
db.prepare(
  "INSERT INTO skills (id, name, description, target, source_path, updated_at) VALUES ('s-1', 'review', 'Review PRs', 'both', ?, ?)",
).run(source, T0);
db.prepare(
  `INSERT INTO project_skills (project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude, synced_hash_codex, applied_at, manual)
   VALUES (?, 's-1', 1, 1, 'h', 'h', ?, 1)`,
).run(PROJECT, T0);

// ---- Drive the server -------------------------------------------------------------

function drive(frames) {
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
      resolve(byId);
    });
    for (const f of frames) child.stdin.write(`${JSON.stringify(f)}\n`);
    child.stdin.end();
  });
}

const call = async (name, args) => {
  const byId = await drive([{ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } }]);
  return byId.get(1)?.result?.content?.[0]?.text ?? '';
};
const isLink = (p) => {
  try {
    return lstatSync(p).isSymbolicLink();
  } catch {
    return false;
  }
};
const projectRow = () =>
  db.prepare("SELECT enabled_claude, enabled_codex FROM project_skills WHERE skill_id = 's-1'").get();

const retarget = await call('skills_update', { id: 's-1', target: 'claude' });
check(retarget.includes('those projects keep their engines') && !retarget.includes('re-apply'),
  'skills_update: a target change says the projects keep their engines');
const row = projectRow();
check(row?.enabled_claude === 1 && row?.enabled_codex === 1, 'skills_update: project engines unchanged');
check(links.every(isLink), 'skills_update: project links untouched');

const deleted = await call('skills_delete', { id: 's-1' });
check(deleted.includes('Was applied to 1 project(s): proj'), 'skills_delete: names the project it was applied to');
check(projectRow() === undefined, 'skills_delete: project_skills rows removed');
check(!links.some(isLink) && !existsSync(source), 'skills_delete: links and source removed');

db.close();
rmSync(appData, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
