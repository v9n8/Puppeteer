#!/usr/bin/env node
// `code_index` of the built-in `puppeteer` MCP server (`library.code_index`).
// Ported from AWOG `sidecar/src/codeindex/__tests__/` (mask, parser, resolver,
// index-build, code-index-tool), plus what Puppeteer adds:
//   - only the project is read: never the home folder or a folder above it, a
//     credential store (SECRET_HOME_DIRS, kept equal to guard.rs by a Rust test) or the
//     app data folder; a symlink out of the project is never followed;
//   - in a git repo, .gitignore applies; files over the size cap are skipped;
//   - the tool hides with its group, and answers over stdio for the run's project.
//
// Builds sample repos in a temp folder (HOME points there too). Needs `git`. No network.
//
// Usage: node sidecar-mcp/test-code-index.mjs

import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createResolver, lineAt, lineStarts, maskCode, parseImportClause, parseSource } from './lib/code-index-parse.mjs';
import {
  MAX_FILE_BYTES,
  blastRadius,
  ensureIndex,
  findDefinitions,
  findReferences,
  resetIndexMemo,
  resolveIndexedPath,
  runCodeIndex,
} from './lib/code-index.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SERVER = join(HERE, 'index.mjs');

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  process.stdout.write(`${ok ? '  ok  ' : '  FAIL'} ${label}\n`);
};
const rejects = async (p) => {
  try {
    await p;
    return null;
  } catch (e) {
    return e.message;
  }
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// realpath: macOS tmpdir is a symlink, and the index reports real paths.
const base = realpathSync(mkdtempSync(join(tmpdir(), 'puppeteer-code-index-')));
const home = join(base, 'home');
const appData = join(base, 'appdata');
mkdirSync(home, { recursive: true });
mkdirSync(appData, { recursive: true });
process.env.HOME = home;
process.env.USERPROFILE = home; // os.homedir() on Windows
process.env.PUPPETEER_DB_PATH = join(appData, 'data.db');

const write = (root, rel, body) => {
  mkdirSync(dirname(join(root, rel)), { recursive: true });
  writeFileSync(join(root, rel), Array.isArray(body) ? body.join('\n') : body);
};

// ---- Masking -----------------------------------------------------------------------

const masked = (src) => maskCode(src).masked;
{
  const src = "const a = 'xin chào'\n// chú thích\n/* nhiều\n   dòng */\nconst b = 1\n";
  const out = masked(src);
  check(out.length === src.length && out.split('\n').length === src.split('\n').length, 'mask: same length and lines');
}
check(masked("const a = 'function foo('") === `const a = '${' '.repeat('function foo('.length)}'`, 'mask: blanks a string, keeps its quotes');
check(masked('a // function ghost()') === 'a                    ', 'mask: line comment');
check(masked('a /* function ghost() */ b') === 'a                        b', 'mask: block comment');
{
  const out = masked('const s = `xin ${greet(name)} chào`');
  check(out.includes('greet(name)') && !out.includes('chào'), 'mask: keeps ${…} code in a template');
  const nested = masked('const s = `a ${ inner(`b ${deep(x)} c`) } d`');
  check(nested.includes('inner(') && nested.includes('deep(x)') && !nested.includes('d`'), 'mask: nested templates');
}
check(masked('const re = /foo(bar)/g') === `const re = /${' '.repeat('foo(bar)'.length)}/g`, 'mask: regex literal');
check(masked('const x = a / b(c) / d').includes('b(c)'), 'mask: a division is not a regex');
check(masked('const x = a / b\nconst y = call(1)\n').includes('call(1)'), 'mask: an unclosed regex stays on its line');
{
  const src = "import { z } from 'zod'";
  check(maskCode(src).strings.get(src.indexOf("'zod'")) === 'zod', 'mask: records short strings by quote offset');
}
check(masked('const a = "hở\nconst b = call(1)\n').includes('call(1)'), 'mask: an unclosed string ends at its line');
{
  let threw = false;
  try {
    maskCode('const a = `chưa đóng');
  } catch {
    threw = true;
  }
  check(!threw, 'mask: survives an unclosed template');
}
{
  const starts = lineStarts('a\nbb\nccc');
  check(lineAt(starts, 0) === 1 && lineAt(starts, 2) === 2 && lineAt(starts, 5) === 3 && lineAt(starts, 8) === 3, 'lineAt: 1-based lines');
}

// ---- Parsing -----------------------------------------------------------------------

{
  const { decls } = parseSource(
    [
      'export function alpha() {}',
      'function beta() {}',
      'export class Gamma {}',
      'export interface Delta { x: number }',
      'export type Epsilon = string',
      'export enum Zeta { A }',
      'export const eta = 3',
      'const theta = (x: number) => x',
    ].join('\n'),
    'a.ts',
  );
  const by = new Map(decls.map((d) => [d.name, d]));
  check(by.get('alpha')?.kind === 'function' && by.get('alpha').exported && by.get('alpha').line === 1, 'parse: exported function');
  check(by.get('beta')?.kind === 'function' && !by.get('beta').exported && by.get('beta').line === 2, 'parse: local function');
  check(
    by.get('Gamma')?.kind === 'class' && by.get('Delta')?.kind === 'interface' && by.get('Epsilon')?.kind === 'type' && by.get('Zeta')?.kind === 'enum',
    'parse: class, interface, type, enum',
  );
  check(by.get('eta')?.kind === 'const' && by.get('theta')?.kind === 'function', 'parse: a const arrow is a function');
}
{
  const { decls } = parseSource(
    ['export class Runner {', '  private state = 1', '  async start(id: string) {', '    if (this.state) { return }', '  }', '  handle = async (x: number) => x', '}'].join('\n'),
    'a.ts',
  );
  const start = decls.find((d) => d.name === 'start');
  check(start?.kind === 'method' && start.container === 'Runner' && start.line === 3, 'parse: class method with its class');
  check(decls.find((d) => d.name === 'handle')?.kind === 'method', 'parse: arrow property is a method');
  check(!decls.some((d) => d.name === 'if'), 'parse: `if (` in a method body is no method');
}
{
  const names = parseSource(
    [
      'const prompt = `Gọi hàm như sau: function ghostOne() {}`',
      "const other = 'export class GhostTwo {}'",
      '// export function ghostThree() {}',
      '/* export interface GhostFour {} */',
    ].join('\n'),
    'a.ts',
  ).decls.map((d) => d.name);
  check(!['ghostOne', 'GhostTwo', 'ghostThree', 'GhostFour'].some((n) => names.includes(n)), 'parse: no ghost declaration from strings or comments');
}
check(parseSource('function inner() {}\nexport { inner }\n', 'a.ts').decls.find((d) => d.name === 'inner')?.exported === true, 'parse: `export { x }` marks x exported');
check(!parseSource('const { alpha, beta } = require("x")\n', 'a.ts').decls.some((d) => d.name === 'alpha'), 'parse: known limit, destructuring declares nothing');
{
  const { imports } = parseSource(
    [
      "import { a, b as c } from './rel.js'",
      "import type { T } from '~/types'",
      "import def from 'pkg'",
      "import * as ns from 'node:fs'",
      "import './side-effect.js'",
      "export { x } from './reexport.js'",
      "export * from './star.js'",
      "const lazy = await import('./dyn.js')",
    ].join('\n'),
    'a.ts',
  );
  const specs = imports.map((i) => i.spec);
  const by = (s) => imports.find((i) => i.spec === s);
  check(['./rel.js', '~/types', 'pkg', 'node:fs', './side-effect.js', './reexport.js', './star.js', './dyn.js'].every((s) => specs.includes(s)), 'parse: every import form');
  check(same(by('./rel.js')?.names, ['a', 'b']), 'parse: imported names');
  check(by('pkg')?.external === true && by('~/types')?.external === false, 'parse: package vs alias');
  check(by('./reexport.js')?.kind === 'reexport' && by('./dyn.js')?.kind === 'dynamic', 'parse: reexport and dynamic kinds');
}
check(!parseSource("const doc = `import x from 'ghost-pkg'`\n", 'a.ts').imports.some((i) => i.spec === 'ghost-pkg'), 'parse: no import from inside a string');
check(
  same(parseImportClause('{ a, b as c }'), ['a', 'b']) && same(parseImportClause('Def, { a }'), ['a', 'default']) &&
    same(parseImportClause('* as ns'), ['*']) && same(parseImportClause('Def'), ['default']),
  'parseImportClause: local names',
);
{
  const { refs } = parseSource(['function target() {}', 'if (true) {', '  target()', '}', 'other.target()'].join('\n'), 'a.ts');
  check(same(refs.target, [3, 5]) && refs.if === undefined, 'parse: call sites with lines, no keywords');
}
check(parseSource('export function solo() {}\n', 'a.ts').refs.solo === undefined, 'parse: a declaration line is no call site');
check(same(parseSource('list.map(f)\nmap(other)\n', 'a.ts').refs.map, [2]), 'parse: built-in member calls dropped, bare call kept');
check(same(parseSource('x.toString()\ntoString(y)\n', 'a.ts').refs.toString, [2]), "parse: a name of Object's prototype is a plain name");
{
  const sfc = ['<template>', '  <SessionList :items="rows" />', '</template>', '', '<script setup lang="ts">', "import { useTheme } from '~/composables/useTheme'", 'const rows = compute()', '</script>'].join('\n');
  const { decls, imports, refs } = parseSource(sfc, 'components/X.vue');
  check(decls.find((d) => d.name === 'rows')?.line === 7 && imports[0]?.spec === '~/composables/useTheme' && imports[0].line === 6, 'parse: Vue <script> keeps file lines');
  check(same(refs.SessionList, [2]), 'parse: Vue <template> component tags are references');
}

// ---- Resolver ----------------------------------------------------------------------

{
  const r = createResolver([
    'src/a/one.ts',
    'src/a/two.ts',
    'src/b/index.ts',
    'src/c/thing.vue',
    'apps/ui/composables/useTheme.ts',
    'apps/ui/pages/home.vue',
    'packages/lib/composables/useTheme.ts',
  ]);
  check(r.resolve('src/a/one.ts', './two.js') === 'src/a/two.ts', 'resolve: NodeNext .js → .ts');
  check(r.resolve('src/a/one.ts', './two') === 'src/a/two.ts' && r.resolve('src/a/one.ts', '../b/index.js') === 'src/b/index.ts', 'resolve: relative without extension');
  check(r.resolve('src/a/one.ts', '../b') === 'src/b/index.ts', 'resolve: folder → index');
  check(r.resolve('src/a/one.ts', '../c/thing.vue') === 'src/c/thing.vue', 'resolve: .vue');
  check(r.resolve('apps/ui/pages/home.vue', '~/pages/home.vue') === 'apps/ui/pages/home.vue', 'resolve: ~/ alias by suffix');
  check(r.resolve('apps/ui/pages/home.vue', '~/composables/useTheme') === null, 'resolve: an ambiguous suffix resolves to nothing');
  check(['zod', 'node:fs', '#imports'].every((s) => r.resolve('src/a/one.ts', s) === null), 'resolve: packages are outside');
  check(r.resolve('src/a/one.ts', '../../outside/x.js') === null, 'resolve: nothing outside the indexed set');
}

// ---- Building and querying a sample repo (walk) --------------------------------------

const repo = join(base, 'repo');
write(repo, 'src/core/engine.ts', ['export function runEngine(id: string) {', '  return prepare(id)', '}', 'function prepare(id: string) {', '  return id', '}', 'export class Engine {', '  start() {', '    return runEngine("x")', '  }', '}']);
write(repo, 'src/core/index.ts', ["export { runEngine } from './engine.js'", '']);
write(repo, 'src/app/caller.ts', ["import { runEngine } from '../core/engine.js'", 'export function boot() {', '  return runEngine("boot")', '}']);
write(repo, 'src/app/deep.ts', ["import { boot } from './caller.js'", 'export const go = () => boot()']);
// No import statement: an auto-imported use.
write(repo, 'src/ui/Panel.vue', ['<template>', '  <div>{{ runEngine }}</div>', '</template>', '<script setup lang="ts">', 'const value = runEngine("auto")', '</script>']);
write(repo, 'node_modules/dep/index.js', 'export function fromDependency() {}\n');
write(repo, 'README.md', 'function notCode() {}\n');

{
  resetIndexMemo();
  const { index, result } = await ensureIndex(repo, { force: true });
  check(index.source === 'walk', 'build: a non-git folder is walked');
  check(
    same(index.files.map((f) => f.path), ['src/app/caller.ts', 'src/app/deep.ts', 'src/core/engine.ts', 'src/core/index.ts', 'src/ui/Panel.vue']),
    'build: only sources, node_modules skipped',
  );
  check(result.parsed === 5 && result.reused === 0, 'build: first pass parses every file');
  const cached = await ensureIndex(repo);
  check(cached.index === index && cached.result.parsed === 0, 'build: a call right after reuses the index as is');
}

const idx = (await ensureIndex(repo, { force: true })).index;
{
  const res = await findDefinitions(idx, 'runEngine');
  const h = res.hits[0];
  check(h?.path === 'src/core/engine.ts' && h.line === 1 && h.kind === 'function' && h.exported, 'define: path, line, kind');
  check(h?.excerpt.includes('export function runEngine'), 'define: excerpt read back from disk');
  const near = await findDefinitions(idx, 'runengine');
  check(near.hits.length === 0 && near.suggestions.includes('runEngine'), 'define: suggests a near name');
}
{
  const res = await findReferences(idx, 'runEngine');
  const places = res.hits.map((h) => `${h.path}:${h.line}`);
  check(places.includes('src/app/caller.ts:3') && places.includes('src/core/engine.ts:9'), 'refs: call sites across files');
  check(res.definedAt.includes('src/core/engine.ts:1'), 'refs: with the declaration');
}
{
  const r = blastRadius(idx, 'src/core/engine.ts', 2);
  check(same(r.direct, ['src/app/caller.ts', 'src/core/index.ts']) && r.transitive.includes('src/app/deep.ts'), 'blast: follows import edges and .js specifiers');
  check(blastRadius(idx, 'src/core/engine.ts', 1).symbolUsers.includes('src/ui/Panel.vue'), 'blast: finds auto-import users by symbol');
}
check(
  resolveIndexedPath(idx, 'src/core/engine.ts') === 'src/core/engine.ts' &&
    resolveIndexedPath(idx, join(repo, 'src/core/engine.ts')) === 'src/core/engine.ts' &&
    resolveIndexedPath(idx, 'core/engine.ts') === 'src/core/engine.ts' &&
    resolveIndexedPath(idx, 'nope/missing.ts') === null,
  'path: relative, absolute or unique suffix',
);

// Incremental: past the in-memory window, only a file whose mtime moved is parsed again.
{
  const future = new Date(Date.now() + 20_000);
  utimesSync(join(repo, 'src/app/deep.ts'), future, future);
  const realNow = Date.now;
  Date.now = () => realNow() + 60_000;
  try {
    const again = await ensureIndex(repo);
    check(again.result.parsed === 1 && again.result.reused === 4, 'build: only the changed file is parsed again');
  } finally {
    Date.now = realNow;
  }
}

// ---- Tool answers --------------------------------------------------------------------

{
  resetIndexMemo();
  const define = await runCodeIndex(repo, { action: 'define', symbol: 'runEngine' });
  check(define.includes('src/core/engine.ts:1') && define.includes('export function runEngine') && !define.includes('return prepare'), 'tool: define gives path:line and one line');
  const refs = await runCodeIndex(repo, { action: 'refs', symbol: 'runEngine' });
  check(refs.includes('src/app/caller.ts:3') && refs.includes('declared at src/core/engine.ts:1'), 'tool: refs tells uses from the declaration');
  const blast = await runCodeIndex(repo, { action: 'blast', path: 'core/engine.ts' });
  check(blast.includes('imported directly by') && blast.includes('src/app/caller.ts'), 'tool: blast');
  const none = await runCodeIndex(repo, { action: 'refs', symbol: 'khongCoThat' });
  check(none.includes('No indexed reference') && /text search|search the text/.test(none), 'tool: "not found" is not "does not exist"');
  const status = await runCodeIndex(repo, { action: 'status' });
  check(status.includes('5 source files') && status.includes('listed via walk') && status.includes('.gitignore was not applied'), 'tool: status tells coverage and the walk');
  check(/needs `symbol`/.test(await rejects(runCodeIndex(repo, { action: 'define' }))), 'tool: define without symbol is an error');
  check(/needs `path`/.test(await rejects(runCodeIndex(repo, { action: 'blast' }))), 'tool: blast without path is an error');
  check(/`action` must be/.test(await rejects(runCodeIndex(repo, { action: 'grep' }))), 'tool: unknown action is an error');
}

// ---- Git repo: .gitignore, symlinks out, size cap ----------------------------------

const gitRepo = join(base, 'git-repo');
const outside = join(base, 'outside');
write(outside, 'secret.ts', 'export function outsideSecret() {}\n');
write(gitRepo, 'src/main.ts', ["import { helper } from './util'", 'helper()']);
write(gitRepo, 'src/util.ts', 'export function helper() {}\n');
write(gitRepo, 'gen/ignored.ts', 'export function ignoredThing() {}\n');
write(gitRepo, '.gitignore', 'gen/\n');
write(gitRepo, 'big.ts', `export function bigOne() {}\n${'// x\n'.repeat(Math.ceil(MAX_FILE_BYTES / 5) + 10)}`);
symlinkSync(join(outside, 'secret.ts'), join(gitRepo, 'src/leak.ts'));
symlinkSync(outside, join(gitRepo, 'linked'));
execFileSync('git', ['init', '-q'], { cwd: gitRepo });
{
  resetIndexMemo();
  const { index } = await ensureIndex(gitRepo, { force: true });
  const files = index.files.map((f) => f.path);
  check(index.source === 'git', 'git: listed by git ls-files');
  check(!files.includes('gen/ignored.ts'), 'git: .gitignore applies');
  check(!files.includes('src/leak.ts') && !files.some((f) => f.startsWith('linked/')), 'git: a symlink out of the project is not read');
  check(!files.includes('big.ts') && index.skipped >= 1, 'git: a file over the size cap is skipped');
  check(same(files, ['src/main.ts', 'src/util.ts']), 'git: the sources are indexed');
  check((await findDefinitions(index, 'outsideSecret')).hits.length === 0, 'git: nothing from outside is searchable');
}

// ---- Roots the index refuses ---------------------------------------------------------

check(/home folder/.test(await rejects(ensureIndex(home))), 'root: refuses the home folder');
check(/home folder/.test(await rejects(ensureIndex(base))), 'root: refuses a folder above home');
mkdirSync(join(home, '.ssh/proj'), { recursive: true });
check(/credentials/.test(await rejects(ensureIndex(join(home, '.ssh/proj')))), 'root: refuses a project inside ~/.ssh');
mkdirSync(join(home, 'Library/Keychains/k'), { recursive: true });
check(/credentials/.test(await rejects(ensureIndex(join(home, 'Library/Keychains/k')))), 'root: refuses the Keychains folder');
mkdirSync(join(appData, 'skills'), { recursive: true });
check(/app data/.test(await rejects(ensureIndex(join(appData, 'skills')))), "root: refuses the app data folder (PUPPETEER_DB_PATH's)");
mkdirSync(join(base, 'support/io.github.v9n8.puppeteer.dev/x'), { recursive: true });
check(/app data/.test(await rejects(ensureIndex(join(base, 'support/io.github.v9n8.puppeteer.dev/x')))), "root: refuses another build's app data");
check(/absolute/.test(await rejects(ensureIndex('relative/path'))), 'root: refuses a relative path');
check(/No current project/.test(await rejects(ensureIndex(''))), 'root: needs a project');

// A project that holds a store (here ~/Library) indexes around it.
{
  const lib = join(home, 'Library');
  write(lib, 'Keychains/k/keys.js', 'export function keychainThing() {}\n');
  write(lib, 'Code/tool.js', 'export function libraryTool() {}\n');
  resetIndexMemo();
  const { index } = await ensureIndex(lib, { force: true });
  check(same(index.files.map((f) => f.path), ['Code/tool.js']), 'root: a store inside the project is skipped');
}

// ---- Over stdio ------------------------------------------------------------------------

function rpc(env, requests) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [SERVER], { env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (d) => (out += d));
    child.on('error', reject);
    child.on('close', () => {
      const byId = new Map(out.split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l)).map((m) => [m.id, m]));
      resolve(requests.map((_, i) => byId.get(i + 1)));
    });
    requests.forEach(([method, params], i) => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: i + 1, method, params })}\n`));
    child.stdin.end();
  });
}
const text = (reply) => reply?.result?.content?.[0]?.text ?? '';
{
  const [list, define, refused] = await rpc({ PUPPETEER_PROJECT_PATH: repo, PUPPETEER_MCP_DISABLED_GROUPS: '' }, [
    ['tools/list', {}],
    ['tools/call', { name: 'code_index', arguments: { action: 'define', symbol: 'boot' } }],
    ['tools/call', { name: 'code_index', arguments: { action: 'refs' } }],
  ]);
  const tool = list?.result?.tools?.find((t) => t.name === 'code_index');
  check(!!tool && same(tool.inputSchema.required, ['action']), 'stdio: code_index listed');
  check(text(define).includes('src/app/caller.ts:2'), 'stdio: define answers for the run project');
  check(refused?.result?.isError === true && /needs `symbol`/.test(text(refused)), 'stdio: a bad call is a tool error');

  const [hiddenList, hiddenCall] = await rpc({ PUPPETEER_PROJECT_PATH: repo, PUPPETEER_MCP_DISABLED_GROUPS: 'code_index' }, [
    ['tools/list', {}],
    ['tools/call', { name: 'code_index', arguments: { action: 'status' } }],
  ]);
  check(!hiddenList?.result?.tools?.some((t) => t.name === 'code_index'), 'stdio: hidden with its group');
  check(hiddenCall?.error?.message === 'Unknown tool: code_index', 'stdio: hidden tool is unknown');

  const [atHome] = await rpc({ PUPPETEER_PROJECT_PATH: home, PUPPETEER_MCP_DISABLED_GROUPS: '' }, [
    ['tools/call', { name: 'code_index', arguments: { action: 'status' } }],
  ]);
  check(atHome?.result?.isError === true && /home folder/.test(text(atHome)), 'stdio: a run in the home folder is refused');
}

rmSync(base, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
