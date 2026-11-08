// `code_index`: look code up by symbol in the current project — where a name is
// declared (`define`), where it is called (`refs`), which files a change to a file can
// break (`blast`) — instead of grepping text (`library.code_index`, group `code_index`
// in lib/feature-groups.mjs). Ported from AWOG `sidecar/src/codeindex/{build,query}.ts`
// and `runtime/tools/code-index-tool.ts`; the text side is lib/code-index-parse.mjs.
//
// What gets read:
//   - only the project of the run (PUPPETEER_PROJECT_PATH), never the home folder or a
//     folder above it;
//   - only TS / JS / Vue sources, listed by `git ls-files` (so .gitignore applies), or
//     by a bounded walk that never follows a symlink when the folder is not a repo;
//   - never a file whose real path leaves the project (a symlink out) or lies in a
//     credential store or Puppeteer's app data (SECRET_HOME_DIRS below, the folder of
//     PUPPETEER_DB_PATH). The Secret vault lives in the Keychain, which is one of them.
// Caps on files, file size and build time keep a huge repo from stalling the server.
//
// The index is kept in this process only (AWOG also saves it to disk): the server lives
// as long as the run, and a rebuild re-parses only the files whose mtime or size moved.
// Answers are `path:line` plus one line read back from disk, never whole files.

import fs from 'node:fs';
import fsp from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { createResolver, parseSource } from './code-index-parse.mjs';

// Copies of SECRET_HOME_DIRS and APP_ID_PREFIX in src-tauri/src/remote/guard.rs (the
// test `code_index_denies_the_same_stores` there keeps them equal).
const SECRET_HOME_DIRS = [
  '.puppeteer',
  '.ssh',
  '.aws',
  '.azure',
  '.codex',
  '.gnupg',
  '.kube',
  '.docker',
  '.password-store',
  '.config/gh',
  '.config/glab-cli',
  '.config/gcloud',
  'Library/Keychains',
];
const APP_ID_PREFIX = 'io.github.v9n8.puppeteer';

export const MAX_INDEXED_FILES = 6000;
export const MAX_FILE_BYTES = 512 * 1024;
const BUILD_DEADLINE_MS = 20_000;
const SOURCE_EXT_RE = /\.(?:ts|tsx|mts|cts|js|jsx|mjs|cjs|vue)$/i;
const MINIFIED_RE = /\.min\.[cm]?js$/i;
// Walk only (git applies .gitignore): build output, vendored code, VCS and editor data.
const SKIP_DIRS = new Set(['.git', 'node_modules', 'dist', '.output', '.nuxt', '.next', '.cache', '.turbo', 'coverage', '.venv', 'venv', '__pycache__', 'target', '.idea', '.vscode']);
const MAX_WALK_DEPTH = 8;
const READ_CONCURRENCY = 12;
// A second call within this window reuses the index without a stat of the whole tree.
const MEMO_TTL_MS = 10_000;

const MAX_TOTAL_CHARS = 24 * 1024;
const EXCERPT_MAX_CHARS = 200;
const MAX_DEF_HITS = 30;
const MAX_REF_HITS = 60;
const MAX_BLAST_FILES = 200;
export const MAX_BLAST_DEPTH = 4;
const DEFAULT_BLAST_DEPTH = 2;
const MAX_LISTED_PER_GROUP = 60;

// ---- Where the index may read ------------------------------------------------------

// Lowercase: macOS volumes usually ignore case, and denying more elsewhere is safe.
function under(p, dir) {
  const a = p.toLowerCase();
  const d = dir.toLowerCase().replace(/[\\/]+$/, '');
  return a === d || a.startsWith(d + path.sep);
}

function realOrSelf(p) {
  try {
    return fs.realpathSync.native(p);
  } catch {
    return path.resolve(p);
  }
}

/**
 * The credential stores of `home` and the app data folder, by literal and real path:
 * `{ homes, denies(abs) }`. `abs` must already be a real path.
 */
export function secretPolicy({ home = os.homedir(), appData = appDataDir() } = {}) {
  const homes = [...new Set([path.resolve(home), realOrSelf(home)])];
  const dirs = homes.flatMap((h) => SECRET_HOME_DIRS.flatMap((d) => [path.join(h, d), realOrSelf(path.join(h, d))]));
  if (appData) dirs.push(path.resolve(appData), realOrSelf(appData));
  return {
    homes,
    denies: (abs) =>
      dirs.some((d) => under(abs, d)) || abs.split(path.sep).some((c) => c.toLowerCase().startsWith(APP_ID_PREFIX)),
  };
}

function appDataDir() {
  const db = process.env.PUPPETEER_DB_PATH;
  return db ? path.dirname(db) : null;
}

/** The real path of a project folder the index may cover; throws otherwise. */
export function indexRoot(raw, policy = secretPolicy()) {
  if (!raw) throw new Error('No current project: the code index covers the project of this run.');
  if (!path.isAbsolute(raw)) throw new Error('The code index root must be an absolute path.');
  let real;
  try {
    real = fs.realpathSync.native(raw);
  } catch {
    throw new Error(`Project folder not found: ${raw}`);
  }
  if (!fs.statSync(real).isDirectory()) throw new Error(`Not a folder: ${raw}`);
  if (policy.homes.some((h) => under(h, real))) {
    throw new Error('Refusing to index the home folder or a folder above it: open a project so the index has a repo root.');
  }
  if (policy.denies(real)) throw new Error(`Refusing to index ${real}: it holds credentials or Puppeteer's app data.`);
  return real;
}

// Real path of `rel` under `root`, or null when it leaves the root or lands in a store.
async function safeFile(root, rel, policy) {
  if (rel.split('/').includes('..')) return null;
  let real;
  try {
    real = await fsp.realpath(path.join(root, rel));
  } catch {
    return null;
  }
  if (real === root || !under(real, root) || policy.denies(real)) return null;
  return real;
}

// ---- Listing and building ----------------------------------------------------------

function listViaGit(root) {
  return new Promise((resolve) => {
    execFile(
      'git',
      // A repo's own config must not run a program here (core.fsmonitor is a command).
      ['-c', 'core.fsmonitor=false', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
      { cwd: root, timeout: 10_000, maxBuffer: 64 * 1024 * 1024 },
      (err, stdout) => {
        if (err) return resolve(null);
        const files = [];
        for (const rel of stdout.split('\0')) {
          if (!rel || !SOURCE_EXT_RE.test(rel) || MINIFIED_RE.test(rel)) continue;
          files.push(rel);
          if (files.length >= MAX_INDEXED_FILES) break;
        }
        resolve(files);
      },
    );
  });
}

async function listViaWalk(root, policy) {
  const files = [];
  const queue = [{ rel: '', depth: 0 }];
  while (queue.length && files.length < MAX_INDEXED_FILES) {
    const node = queue.shift();
    let dirents;
    try {
      dirents = await fsp.readdir(path.join(root, node.rel), { withFileTypes: true });
    } catch {
      continue;
    }
    for (const d of dirents) {
      if (d.isSymbolicLink()) continue;
      const rel = node.rel ? `${node.rel}/${d.name}` : d.name;
      if (d.isDirectory()) {
        if (!SKIP_DIRS.has(d.name) && node.depth < MAX_WALK_DEPTH && !policy.denies(path.join(root, rel))) {
          queue.push({ rel, depth: node.depth + 1 });
        }
      } else if (d.isFile() && SOURCE_EXT_RE.test(d.name) && !MINIFIED_RE.test(d.name)) {
        files.push(rel);
        if (files.length >= MAX_INDEXED_FILES) break;
      }
    }
  }
  return files;
}

async function inBatches(items, fn) {
  const out = [];
  for (let i = 0; i < items.length; i += READ_CONCURRENCY) {
    out.push(...(await Promise.all(items.slice(i, i + READ_CONCURRENCY).map(fn))));
  }
  return out;
}

async function build(root, policy, previous) {
  const started = Date.now();
  const fromGit = await listViaGit(root);
  const source = fromGit ? 'git' : 'walk';
  const paths = fromGit ?? (await listViaWalk(root, policy));

  const old = new Map((previous?.files ?? []).map((f) => [f.path, f]));
  const kept = [];
  const todo = [];
  let reused = 0;
  let skipped = 0;

  // Stat first: only files whose mtime or size moved are parsed again.
  const stats = await inBatches(paths, async (rel) => {
    const real = await safeFile(root, rel, policy);
    const st = real ? await fsp.stat(real).catch(() => null) : null;
    return { rel, real, st };
  });
  for (const { rel, real, st } of stats) {
    if (!st || !st.isFile() || st.size > MAX_FILE_BYTES) {
      skipped++;
      continue;
    }
    const before = old.get(rel);
    if (before && before.mtimeMs === st.mtimeMs && before.size === st.size) {
      kept.push(before);
      reused++;
    } else {
      todo.push({ rel, real, st });
    }
  }

  let parsed = 0;
  let deadlineHit = false;
  const done = new Set();
  for (let i = 0; i < todo.length; i += READ_CONCURRENCY) {
    if (Date.now() - started > BUILD_DEADLINE_MS) {
      deadlineHit = true;
      break;
    }
    const results = await Promise.all(
      todo.slice(i, i + READ_CONCURRENCY).map(async ({ rel, real, st }) => {
        try {
          const text = await fsp.readFile(real, 'utf8');
          return { path: rel, mtimeMs: st.mtimeMs, size: st.size, ...parseSource(text, rel) };
        } catch {
          return null;
        }
      }),
    );
    for (const file of results) {
      if (!file) {
        skipped++;
        continue;
      }
      kept.push(file);
      done.add(file.path);
      parsed++;
    }
  }
  // Out of time: keep the old records of what was not parsed; their mtime still differs,
  // so the next call carries on.
  if (deadlineHit) {
    for (const { rel } of todo) if (!done.has(rel) && old.has(rel)) kept.push(old.get(rel));
  }

  kept.sort((a, b) => a.path.localeCompare(b.path));
  const resolver = createResolver(kept.map((f) => f.path));
  for (const file of kept) for (const imp of file.imports) imp.target = resolver.resolve(file.path, imp.spec) ?? undefined;

  const index = { root, builtAt: Date.now(), files: kept, skipped, truncated: paths.length >= MAX_INDEXED_FILES, source };
  return { index, result: { parsed, reused, skipped, deadlineHit, durationMs: Date.now() - started } };
}

const memo = new Map();
// Two calls at once on one root share one build.
const inFlight = new Map();

/** The up-to-date index of the project folder `raw`, built or refreshed as needed. */
export async function ensureIndex(raw, { force = false } = {}) {
  const policy = secretPolicy();
  const root = indexRoot(raw, policy);
  const cached = memo.get(root);
  if (!force && cached && Date.now() - cached.checkedAt < MEMO_TTL_MS) {
    return { index: cached.index, result: { parsed: 0, reused: cached.index.files.length, skipped: cached.index.skipped, deadlineHit: false } };
  }
  if (!force && inFlight.has(root)) return inFlight.get(root);
  const task = build(root, policy, force ? null : cached?.index ?? null)
    .then((built) => {
      memo.set(root, { index: built.index, checkedAt: Date.now() });
      return built;
    })
    .finally(() => inFlight.delete(root));
  inFlight.set(root, task);
  return task;
}

/** Tests only: forget every index. */
export function resetIndexMemo() {
  memo.clear();
  inFlight.clear();
}

// ---- Queries -----------------------------------------------------------------------

// Excerpts are read back at query time: the index stays small and the line is never
// older than the file.
function lineReader(root) {
  const policy = secretPolicy();
  const cache = new Map();
  return async (rel, line) => {
    if (!cache.has(rel)) {
      const real = await safeFile(root, rel, policy);
      cache.set(rel, real ? (await fsp.readFile(real, 'utf8').catch(() => null))?.split('\n') ?? null : null);
    }
    const text = cache.get(rel)?.[line - 1]?.trim() ?? '';
    return text.length > EXCERPT_MAX_CHARS ? `${text.slice(0, EXCERPT_MAX_CHARS)}…` : text;
  };
}

const KIND_RANK = { function: 0, class: 0, method: 1, interface: 2, type: 2, enum: 2, const: 3 };
const rank = (d) => (d.exported ? 0 : 10) + KIND_RANK[d.kind];

// Same name in another case, or a name containing it: "did you mean" beats "nothing".
function suggestNames(index, name) {
  const lower = name.toLowerCase();
  const found = new Set();
  for (const file of index.files) {
    for (const d of file.decls) {
      if (found.size >= 8) return [...found];
      const c = d.name.toLowerCase();
      if (c === lower || (name.length >= 4 && c.includes(lower))) found.add(d.name);
    }
  }
  return [...found];
}

export async function findDefinitions(index, name, { pathFilter } = {}) {
  const matches = [];
  for (const file of index.files) {
    if (pathFilter && !file.path.includes(pathFilter)) continue;
    for (const decl of file.decls) if (decl.name === name) matches.push({ file, decl });
  }
  matches.sort((a, b) => rank(a.decl) - rank(b.decl) || a.file.path.localeCompare(b.file.path) || a.decl.line - b.decl.line);
  const read = lineReader(index.root);
  const hits = [];
  for (const { file, decl } of matches.slice(0, MAX_DEF_HITS)) {
    hits.push({ path: file.path, ...decl, excerpt: await read(file.path, decl.line) });
  }
  return { hits, total: matches.length, suggestions: matches.length ? [] : suggestNames(index, name) };
}

export async function findReferences(index, name, { pathFilter } = {}) {
  const flat = [];
  const definedAt = [];
  let files = 0;
  for (const file of index.files) {
    for (const decl of file.decls) if (decl.name === name) definedAt.push(`${file.path}:${decl.line}`);
    if (pathFilter && !file.path.includes(pathFilter)) continue;
    const lines = Object.hasOwn(file.refs, name) ? file.refs[name] : null;
    if (!lines?.length) continue;
    files++;
    for (const line of lines) flat.push({ path: file.path, line });
  }
  const read = lineReader(index.root);
  const hits = [];
  for (const hit of flat.slice(0, MAX_REF_HITS)) hits.push({ ...hit, excerpt: await read(hit.path, hit.line) });
  return { hits, total: flat.length, files, definedAt: definedAt.slice(0, MAX_DEF_HITS) };
}

/** An absolute path, a root-relative path or a unique suffix → the indexed path, or null. */
export function resolveIndexedPath(index, input) {
  const raw = String(input ?? '').trim().replace(/\\/g, '/');
  if (!raw) return null;
  const candidates = [raw.replace(/^\.\//, '')];
  if (path.isAbsolute(raw)) {
    for (const abs of [path.resolve(raw), realOrSelf(raw)]) {
      const rel = path.relative(index.root, abs).split(path.sep).join('/');
      if (rel && !rel.startsWith('..')) candidates.unshift(rel);
    }
  }
  for (const c of candidates) if (index.files.some((f) => f.path === c)) return c;
  const tail = index.files.filter((f) => f.path.endsWith(`/${candidates[candidates.length - 1]}`));
  return tail.length === 1 ? tail[0].path : null;
}

export function blastRadius(index, file, depth) {
  const wanted = Math.min(Math.max(1, Math.floor(depth) || 1), MAX_BLAST_DEPTH);
  const importedBy = new Map();
  for (const f of index.files) {
    for (const imp of f.imports) {
      if (!imp.target) continue;
      const bucket = importedBy.get(imp.target) ?? [];
      if (!bucket.includes(f.path)) bucket.push(f.path);
      importedBy.set(imp.target, bucket);
    }
  }
  const seen = new Set([file]);
  const direct = [];
  const transitive = [];
  let frontier = [file];
  let truncated = false;
  for (let level = 1; level <= wanted && frontier.length && !truncated; level++) {
    const next = [];
    for (const node of frontier) {
      for (const parent of importedBy.get(node) ?? []) {
        if (seen.has(parent)) continue;
        if (seen.size >= MAX_BLAST_FILES) {
          truncated = true;
          break;
        }
        seen.add(parent);
        next.push(parent);
        (level === 1 ? direct : transitive).push(parent);
      }
      if (truncated) break;
    }
    frontier = next;
  }
  // Users without an import edge (auto-imports): they name a symbol the file exports.
  const self = index.files.find((f) => f.path === file);
  const exported = new Set(self?.decls.filter((d) => d.exported).map((d) => d.name) ?? []);
  const symbolUsers = [];
  if (exported.size) {
    for (const f of index.files) {
      if (seen.has(f.path)) continue;
      if (Object.keys(f.refs).some((n) => exported.has(n))) symbolUsers.push(f.path);
      if (symbolUsers.length >= MAX_BLAST_FILES) {
        truncated = true;
        break;
      }
    }
  }
  return { path: file, direct: direct.sort(), transitive: transitive.sort(), symbolUsers: symbolUsers.sort(), depth: wanted, truncated };
}

// ---- Tool --------------------------------------------------------------------------

// Lines up to a byte budget, with a hint to narrow the query when some are dropped.
function clamp(lines, hint) {
  const kept = [];
  let total = 0;
  for (const l of lines) {
    const line = l.length > 1000 ? `${l.slice(0, 1000)}…` : l;
    if (total + line.length + 1 > MAX_TOTAL_CHARS) return `${kept.join('\n')}\n…(truncated — ${hint})`;
    kept.push(line);
    total += line.length + 1;
  }
  return kept.join('\n');
}

function group(label, paths) {
  if (!paths.length) return [];
  const shown = paths.slice(0, MAX_LISTED_PER_GROUP);
  const more = paths.length - shown.length;
  return [`${label} (${paths.length}):`, ...shown.map((p) => `  ${p}`), ...(more > 0 ? [`  …and ${more} more`] : [])];
}

/** The tool's answer for the project folder `root`; throws on a bad call or a refused root. */
export async function runCodeIndex(root, params = {}) {
  const action = params.action;
  if (!['define', 'refs', 'blast', 'status'].includes(action)) throw new Error('`action` must be define, refs, blast or status.');
  const { index, result } = await ensureIndex(root);

  const caveats = [];
  if (index.truncated) caveats.push('The repository is larger than the index cap, so some files are NOT covered.');
  if (result.deadlineHit) caveats.push('Indexing hit its time budget; part of the tree is from a previous pass.');
  if (index.source === 'walk') caveats.push('This folder is not a git repository, so .gitignore was not applied.');
  const withCaveats = (text) => (caveats.length ? `${text}\n\nNote: ${caveats.join(' ')}` : text);

  if (action === 'status') {
    let decls = 0;
    let names = 0;
    let edges = 0;
    let unresolved = 0;
    for (const f of index.files) {
      decls += f.decls.length;
      names += Object.keys(f.refs).length;
      for (const imp of f.imports) {
        if (imp.target) edges++;
        else if (!imp.external) unresolved++;
      }
    }
    const age = Math.max(0, Math.round((Date.now() - index.builtAt) / 1000));
    return withCaveats(
      [
        `Code index for ${index.root}`,
        `${index.files.length} source files · ${decls} declarations · ${names} indexed names`,
        `${edges} resolved internal import edges (${unresolved} unresolved), ${index.skipped} files skipped`,
        `listed via ${index.source}, refreshed ${age}s ago, kept in memory for this session`,
      ].join('\n'),
    );
  }

  if (action === 'blast') {
    if (!params.path) throw new Error('blast needs `path` — the file you intend to change.');
    const target = resolveIndexedPath(index, params.path);
    if (!target) throw new Error(`No indexed file matches "${params.path}". Pass a project-relative path, or a suffix that is unique.`);
    const r = blastRadius(index, target, params.depth ?? DEFAULT_BLAST_DEPTH);
    const total = r.direct.length + r.transitive.length + r.symbolUsers.length;
    if (!total) {
      return withCaveats(`Nothing in the index imports or references ${target}. It may be an entry point, dead code, or reached only dynamically.`);
    }
    const lines = [
      `Changing ${target} can affect ${total} file(s) (import depth ${r.depth}):`,
      ...group('imported directly by', r.direct),
      ...group(`reachable through further imports (depth 2..${r.depth})`, r.transitive),
      ...group('reference a symbol it exports without importing it (auto-import / same-name)', r.symbolUsers),
    ];
    if (r.truncated) lines.push('…blast radius hit its file cap; the real reach is larger.');
    return withCaveats(clamp(lines, 'lower `depth`'));
  }

  const symbol = String(params.symbol ?? '').trim();
  if (!symbol) throw new Error(`${action} needs \`symbol\` — an exact symbol name.`);
  const pathFilter = String(params.path ?? '').trim() || undefined;

  if (action === 'define') {
    const res = await findDefinitions(index, symbol, { pathFilter });
    if (!res.hits.length) {
      const hint = res.suggestions.length
        ? ` Similar indexed names: ${res.suggestions.join(', ')}.`
        : ' It may be created by destructuring, generated, or come from a dependency — search the text instead.';
      return withCaveats(`No indexed declaration of "${symbol}".${hint}`);
    }
    const lines = res.hits.map((h) => {
      const tags = [h.exported ? 'exported' : 'local', h.kind, h.container ? `in ${h.container}` : ''].filter(Boolean).join(' ');
      return `${h.path}:${h.line}  [${tags}]  ${h.excerpt}`;
    });
    const head = `${res.total} declaration(s) named "${symbol}"${res.total > res.hits.length ? ` (showing ${res.hits.length})` : ''}:`;
    return withCaveats(clamp([head, ...lines], 'pass `path` to narrow by directory'));
  }

  const res = await findReferences(index, symbol, { pathFilter });
  if (!res.total) {
    const defined = res.definedAt.length
      ? ` It IS declared at ${res.definedAt.join(', ')}, so it may be unused — or used in a way the index does not see (destructured, re-exported under another name, called through a variable). Confirm with a text search before deleting anything.`
      : ' The index has no declaration of it either — check the spelling, or search the text.';
    return withCaveats(`No indexed reference to "${symbol}".${defined}`);
  }
  const head =
    `${res.total} reference(s) to "${symbol}" across ${res.files} file(s)` +
    `${res.total > res.hits.length ? ` (showing ${res.hits.length})` : ''}` +
    `${res.definedAt.length ? `; declared at ${res.definedAt.join(', ')}` : ''}:`;
  return withCaveats(clamp([head, ...res.hits.map((h) => `${h.path}:${h.line}  ${h.excerpt}`)], 'pass `path` to narrow by directory'));
}

export function codeIndexTools() {
  return {
    code_index: {
      description:
        'Look up code in the current project by SYMBOL instead of by text. Answers three questions a text search cannot: where is this declared (define), everywhere it is called or used (refs), and which files break if I change this file (blast). ' +
        'It follows import edges, so it resolves NodeNext ".js" specifiers to the real ".ts" file, "~/" and "@/" aliases, folder index files and re-exports; for auto-imports (a composable or Vue component used with no import statement) blast still finds the users through the symbol name. ' +
        'Covers TypeScript, JavaScript and Vue files only. Results are path:line plus one short line of context — never whole files. ' +
        'Reach for this FIRST when the question is about relationships ("who calls X", "what depends on Y", "is this still used"), and search the text when the question is really about text (a log string, a comment, a config value). ' +
        'LIMITS, state them if they matter: the index is built by a lightweight parser (regex + brace counting), NOT the TypeScript compiler. It sees top-level functions/classes/interfaces/types/enums/consts and class methods, but NOT symbols created by destructuring (`const { a } = x`), object-literal methods, or anything generated at runtime. It matches names EXACTLY and cannot tell two same-named symbols apart. So "refs found nothing" means "the index has no record", not "nothing calls it" — confirm with a text search before deleting anything.',
      inputSchema: {
        type: 'object',
        properties: {
          action: {
            type: 'string',
            enum: ['define', 'refs', 'blast', 'status'],
            description: 'define = where a symbol is declared; refs = every place that calls/uses it; blast = which files break if you change a given file; status = index coverage and freshness.',
          },
          symbol: { type: 'string', description: 'Exact, case-sensitive symbol name for define/refs (e.g. "createResolver"). Not a pattern.' },
          path: {
            type: 'string',
            description: 'For blast: the file to change (project-relative, absolute, or a unique path suffix like "lib/store.mjs"). For define/refs: optional substring filter on the file path.',
          },
          depth: {
            type: 'integer',
            minimum: 1,
            maximum: MAX_BLAST_DEPTH,
            description: `For blast: how many import hops to follow (default ${DEFAULT_BLAST_DEPTH}, max ${MAX_BLAST_DEPTH}).`,
          },
        },
        required: ['action'],
      },
      handler: (a) => runCodeIndex(process.env.PUPPETEER_PROJECT_PATH, a),
    },
  };
}
