#!/usr/bin/env node
// `file_tree` of the built-in `puppeteer` MCP server: `subpath` stays inside the
// current project (PUPPETEER_PROJECT_PATH). A sibling folder that shares the
// project's name as a prefix (`proj` vs `proj-secrets`) and a symlink out of the
// project are both refused; real subfolders still list.
//
// Usage: node sidecar-mcp/test-file-tree.mjs

import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileTree } from './lib/project.mjs';

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  process.stdout.write(`${ok ? '  ok  ' : '  FAIL'} ${label}\n`);
};
const run = (subpath) => {
  try {
    return fileTree({ subpath });
  } catch (e) {
    return `Error: ${e.message}`;
  }
};

const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'puppeteer-file-tree-')));
const proj = join(tmp, 'proj');
const secrets = join(tmp, 'proj-secrets');
mkdirSync(join(proj, 'src'), { recursive: true });
mkdirSync(secrets);
writeFileSync(join(proj, 'src', 'main.js'), '');
writeFileSync(join(secrets, 'id_rsa'), 'secret');
// 'dir' makes a directory junction on Windows, which needs no admin rights.
symlinkSync(secrets, join(proj, 'link'), 'dir');
process.env.PUPPETEER_PROJECT_PATH = proj;

const sibling = run('../proj-secrets');
check(/escapes/.test(sibling) && !sibling.includes('id_rsa'), `refuses a sibling sharing the root prefix (got: ${sibling.slice(0, 80)})`);
const viaLink = run('link');
check(/escapes/.test(viaLink) && !viaLink.includes('id_rsa'), `refuses a symlink out of the project (got: ${viaLink.slice(0, 80)})`);
check(/escapes/.test(run('..')), 'refuses the parent folder');
check(run('src').includes('main.js'), 'lists a real subfolder');
check(run('').includes('src'), 'lists the project root');
check(/not found/.test(run('missing')), 'reports a missing subfolder');

rmSync(tmp, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
