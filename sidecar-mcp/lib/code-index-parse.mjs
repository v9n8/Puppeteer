// Text side of the code index (lib/code-index.mjs): mask strings and comments, read
// declarations / imports / call sites out of one TS, JS or Vue file, and resolve
// import specifiers to indexed files. Pure: no file system. Ported from AWOG
// `sidecar/src/codeindex/{mask,parser,resolver}.ts`.
//
// Deliberately not the TypeScript compiler: masking + regexes + bounded brace counting.
// It misses destructured names, object-literal methods and runtime-built symbols, and
// cannot tell two symbols of the same name apart. The tool says so to the model, so a
// "no reference found" is never read as "unused".

import path from 'node:path';

// Caps on what one file contributes (a generated file can hold thousands of names).
export const MAX_REF_NAMES_PER_FILE = 800;
export const MAX_REF_LINES_PER_NAME = 40;
export const MAX_DECLS_PER_FILE = 2000;

// ---- Masking -----------------------------------------------------------------------
//
// Strings, comments and regex literals become spaces, so example code inside a prompt
// string yields no ghost declaration. The result has the same length and the same
// newlines as the input: every match offset maps to the right line. `${…}` inside a
// template stays: it is real code.

// Longer than this is never a module specifier.
const MAX_RECORDED_STRING = 256;

// The character before a `/` decides regex or division (classic heuristic; a wrong
// guess is contained by scanning a regex on one line only).
const REGEX_PREV_CHARS = new Set(['(', ',', '=', ':', '[', '!', '&', '|', '?', '{', '}', ';', '+', '-', '*', '%', '~', '^', '<', '>', '\n']);
const REGEX_PREV_WORDS = new Set(['return', 'typeof', 'instanceof', 'in', 'of', 'new', 'delete', 'void', 'case', 'do', 'else', 'yield', 'await', 'throw']);

function isRegexStart(src, i) {
  let j = i - 1;
  while (j >= 0 && (src[j] === ' ' || src[j] === '\t')) j--;
  if (j < 0) return true;
  const ch = src[j];
  if (REGEX_PREV_CHARS.has(ch)) return true;
  if (/[\w$)\]]/.test(ch)) {
    let k = j;
    while (k >= 0 && /[\w$]/.test(src[k])) k--;
    return REGEX_PREV_WORDS.has(src.slice(k + 1, j + 1));
  }
  return true;
}

// Offset after the closing `/`, or -1 when it does not close on the same line (then it
// was a division after all).
function scanRegex(src, i) {
  let j = i + 1;
  let inClass = false;
  while (j < src.length) {
    const c = src[j];
    if (c === '\n') return -1;
    if (c === '\\') {
      j += 2;
      continue;
    }
    if (c === '[') inClass = true;
    else if (c === ']') inClass = false;
    else if (c === '/' && !inClass) return j + 1;
    j++;
  }
  return -1;
}

// Offset after the closing quote; an unclosed string ends at its line.
function scanQuoted(src, i) {
  const quote = src[i];
  let j = i + 1;
  while (j < src.length) {
    const c = src[j];
    if (c === '\\') {
      j += 2;
      continue;
    }
    if (c === '\n') return j;
    if (c === quote) return j + 1;
    j++;
  }
  return src.length;
}

const blank = (text) => text.replace(/[^\n]/g, ' ');

/** `{ masked, strings }`: `strings` maps the offset of an opening quote to its short one-line content. */
export function maskCode(src) {
  const ranges = [];
  const strings = new Map();
  const frames = [{ kind: 'code', depth: 0 }];
  const n = src.length;
  let i = 0;

  while (i < n) {
    const frame = frames[frames.length - 1];
    const c = src[i];

    if (frame.kind === 'template') {
      if (c === '\\') {
        i += 2;
        continue;
      }
      if (c === '`') {
        ranges.push([frame.segStart, i]);
        frames.pop();
        i++;
        continue;
      }
      if (c === '$' && src[i + 1] === '{') {
        ranges.push([frame.segStart, i]);
        frames.push({ kind: 'code', depth: 0 });
        i += 2;
        continue;
      }
      i++;
      continue;
    }

    if (c === '/' && src[i + 1] === '/') {
      const nl = src.indexOf('\n', i);
      const end = nl === -1 ? n : nl;
      ranges.push([i, end]);
      i = end;
      continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      const close = src.indexOf('*/', i + 2);
      const end = close === -1 ? n : close + 2;
      ranges.push([i, end]);
      i = end;
      continue;
    }
    if (c === '"' || c === "'") {
      const end = scanQuoted(src, i);
      const value = src.slice(i + 1, Math.max(i + 1, end - 1));
      if (value.length <= MAX_RECORDED_STRING && !value.includes('\n')) strings.set(i, value);
      ranges.push([i + 1, Math.max(i + 1, end - 1)]);
      i = end;
      continue;
    }
    if (c === '`') {
      frames.push({ kind: 'template', segStart: i + 1 });
      i++;
      continue;
    }
    if (c === '/' && isRegexStart(src, i)) {
      const end = scanRegex(src, i);
      if (end !== -1) {
        ranges.push([i + 1, end - 1]);
        i = end;
        continue;
      }
    }
    if (c === '{') {
      frame.depth++;
    } else if (c === '}') {
      if (frame.depth === 0 && frames.length > 1) {
        frames.pop();
        const parent = frames[frames.length - 1];
        if (parent.kind === 'template') parent.segStart = i + 1;
      } else if (frame.depth > 0) {
        frame.depth--;
      }
    }
    i++;
  }

  // An unclosed template at the end of a broken file: mask the tail.
  const last = frames[frames.length - 1];
  if (last.kind === 'template' && last.segStart < n) ranges.push([last.segStart, n]);

  let masked = '';
  let pos = 0;
  for (const [start, end] of ranges) {
    if (start < pos) continue;
    masked += src.slice(pos, start) + blank(src.slice(start, end));
    pos = end;
  }
  masked += src.slice(pos);
  return { masked, strings };
}

/** Offsets where each line starts, for `lineAt`. */
export function lineStarts(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') starts.push(i + 1);
  return starts;
}

/** 1-based line of `offset`; `starts` comes from `lineStarts` on the same text. */
export function lineAt(starts, offset) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

// ---- Parsing -----------------------------------------------------------------------

// `if (`, `for (`… look like calls once strings are masked.
const NON_CALL_WORDS = new Set([
  'if', 'for', 'while', 'switch', 'catch', 'return', 'function', 'typeof', 'new', 'await', 'do', 'with', 'in', 'of',
  'case', 'delete', 'void', 'yield', 'super', 'import', 'export', 'else', 'throw', 'instanceof', 'constructor', 'class',
  'extends', 'implements', 'as', 'satisfies', 'keyof', 'infer', 'is', 'asserts', 'this', 'and', 'or', 'not',
]);

// Language and runtime globals: in every file, never what someone looks up.
const GLOBAL_STOP = new Set([
  'console', 'Math', 'JSON', 'Object', 'Array', 'String', 'Number', 'Boolean', 'Date', 'Promise', 'Map', 'Set',
  'WeakMap', 'WeakSet', 'Symbol', 'Error', 'TypeError', 'RangeError', 'SyntaxError', 'RegExp', 'Proxy', 'Reflect',
  'BigInt', 'parseInt', 'parseFloat', 'isNaN', 'isFinite', 'setTimeout', 'clearTimeout', 'setInterval',
  'clearInterval', 'setImmediate', 'queueMicrotask', 'structuredClone', 'encodeURIComponent', 'decodeURIComponent',
  'Buffer', 'process', 'globalThis', 'Intl', 'TextEncoder', 'TextDecoder',
]);

// Built-in methods, dropped only as member calls (`x.map(`): a local `map(` stays.
const MEMBER_STOP = new Set([
  'push', 'pop', 'shift', 'unshift', 'map', 'filter', 'forEach', 'reduce', 'slice', 'splice', 'join', 'concat',
  'includes', 'indexOf', 'lastIndexOf', 'find', 'findIndex', 'findLast', 'some', 'every', 'sort', 'reverse', 'flat',
  'flatMap', 'keys', 'values', 'entries', 'has', 'get', 'set', 'add', 'delete', 'clear', 'trim', 'trimStart',
  'trimEnd', 'split', 'replace', 'replaceAll', 'startsWith', 'endsWith', 'toLowerCase', 'toUpperCase', 'padStart',
  'padEnd', 'repeat', 'charAt', 'charCodeAt', 'codePointAt', 'substring', 'match', 'matchAll', 'search', 'test',
  'exec', 'then', 'catch', 'finally', 'toString', 'valueOf', 'toFixed', 'stringify', 'parse', 'log', 'warn', 'error',
  'info', 'debug', 'assign', 'freeze', 'from', 'of', 'isArray', 'now', 'floor', 'ceil', 'round', 'abs', 'min', 'max',
  'random', 'bind', 'call', 'apply', 'json', 'text', 'at', 'localeCompare', 'toISOString', 'getTime',
]);

const FUNCTION_RE = /(?<![\w$.])(export\s+)?(?:default\s+)?(?:declare\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)/g;
const CLASS_RE = /(?<![\w$.])(export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)/g;
const INTERFACE_RE = /(?<![\w$.])(export\s+)?(?:declare\s+)?interface\s+([A-Za-z_$][\w$]*)/g;
const TYPE_RE = /(?<![\w$.])(export\s+)?(?:declare\s+)?type\s+([A-Za-z_$][\w$]*)\s*[<=]/g;
const ENUM_RE = /(?<![\w$.])(export\s+)?(?:declare\s+)?(?:const\s+)?enum\s+([A-Za-z_$][\w$]*)/g;
const VAR_RE = /(?<![\w$.])(export\s+)?(?:declare\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*[:=]/g;

// Kept linear on one long line: without the lookbehind and the cap, a long name is
// rescanned from each of its characters; with two space runs side by side, a long run
// of spaces is retried at every split. A name over 255 characters is no call site.
const REF_RE = /(?<![\w$])([A-Za-z_$][\w$]{0,254})\s*(?:<[^<>()]{0,80}>\s*)?\(/g;

// Class members at depth 1: a method (`name(`) or an arrow property (`name = (…) =>`).
const MEMBER_RE =
  /^\s*(?:(?:public|private|protected|readonly|static|abstract|override|declare|async|get|set)\s+)*\*?\s*#?([A-Za-z_$][\w$]*)\s*\??\s*(?:<[^<>]{0,80}>)?\s*\(/;
const MEMBER_ARROW_RE =
  /^\s*(?:(?:public|private|protected|readonly|static|abstract|override|declare)\s+)*#?([A-Za-z_$][\w$]*)\s*(?::[^=]{0,120})?=\s*(?:async\s*)?\(/;

// An import clause may span lines but never holds a quote or `;`: otherwise a bare
// `import 'x'` would swallow the next statement and take its specifier.
const IMPORT_RE = /(?<![\w$.])import\s+(?:type\s+)?([^;'"`]{0,400}?)\s*from\s*(['"])/g;
const BARE_IMPORT_RE = /(?<![\w$.])import\s*(['"])/g;
const DYNAMIC_IMPORT_RE = /(?<![\w$.])import\s*\(\s*(['"])/g;
const REQUIRE_RE = /(?<![\w$.])require\s*\(\s*(['"])/g;
const EXPORT_BRACE_RE = /(?<![\w$.])export\s+(?:type\s+)?\{([^}]{0,600})\}(\s*from\s*(['"]))?/g;
const EXPORT_STAR_RE = /(?<![\w$.])export\s+\*(?:\s+as\s+[A-Za-z_$][\w$]*)?\s*from\s*(['"])/g;

// A component tag in a Vue <template>: the only trace of an auto-imported component.
const VUE_TAG_RE = /<([A-Z][A-Za-z0-9_]*)/g;

function vueScriptRegions(src) {
  const regions = [];
  const open = /<script\b[^>]*>/gi;
  const lower = src.toLowerCase();
  let m;
  while ((m = open.exec(src)) !== null) {
    const start = m.index + m[0].length;
    const close = lower.indexOf('</script>', start);
    const end = close === -1 ? src.length : close;
    regions.push([start, end]);
    open.lastIndex = end;
  }
  return regions;
}

// Same length and newlines; only `regions` kept.
function keepOnly(src, regions) {
  if (regions.length === 0) return blank(src);
  let out = '';
  let pos = 0;
  for (const [start, end] of regions) {
    out += blank(src.slice(pos, start)) + src.slice(start, end);
    pos = end;
  }
  return out + blank(src.slice(pos));
}

function invertRegions(regions, length) {
  const out = [];
  let pos = 0;
  for (const [start, end] of regions) {
    if (start > pos) out.push([pos, start]);
    pos = end;
  }
  if (pos < length) out.push([pos, length]);
  return out;
}

function isBareSpecifier(spec) {
  if (spec.startsWith('.') || spec.startsWith('/')) return false;
  return !(spec.startsWith('~') || spec.startsWith('@/') || spec.startsWith('@@/'));
}

/** `Default, { a, b as c }` / `* as ns` → the names brought in (`default`, `*` for a namespace). */
export function parseImportClause(clause) {
  const names = [];
  const braced = clause.match(/\{([^}]*)\}/);
  if (braced) {
    for (const part of braced[1].split(',')) {
      const local = part.trim().replace(/^type\s+/, '').split(/\s+as\s+/)[0].trim();
      if (local) names.push(local);
    }
  }
  const head = clause.replace(/\{[^}]*\}/, '').replace(/,/g, ' ').trim();
  if (/\*\s+as\s+[A-Za-z_$][\w$]*/.test(head)) names.push('*');
  const def = head.replace(/\*\s+as\s+[A-Za-z_$][\w$]*/, '').trim();
  if (/^[A-Za-z_$][\w$]*$/.test(def)) names.push('default');
  return names;
}

// `export { a, b as c }` → the local names.
function parseExportList(body) {
  const names = [];
  for (const part of body.split(',')) {
    const local = part.trim().replace(/^type\s+/, '').split(/\s+as\s+/)[0].trim();
    if (local && local !== 'default') names.push(local);
  }
  return names;
}

function specAt(strings, quoteOffset) {
  const value = strings.get(quoteOffset);
  return value ? value : null;
}

function collectImports(masked, strings, starts) {
  const imports = [];
  const push = (spec, offset, names, kind) =>
    imports.push({ spec, line: lineAt(starts, offset), names, kind, external: isBareSpecifier(spec) });
  const each = (re, fn) => {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(masked)) !== null) fn(m, m.index + m[0].length - 1);
  };
  each(IMPORT_RE, (m, q) => {
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, parseImportClause(m[1]), 'import');
  });
  each(BARE_IMPORT_RE, (m, q) => {
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, [], 'import');
  });
  each(DYNAMIC_IMPORT_RE, (m, q) => {
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, [], 'dynamic');
  });
  each(REQUIRE_RE, (m, q) => {
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, [], 'require');
  });
  each(EXPORT_BRACE_RE, (m, q) => {
    if (m[2] === undefined) return; // plain `export { a }`: marks declarations (collectDecls)
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, parseExportList(m[1]), 'reexport');
  });
  each(EXPORT_STAR_RE, (m, q) => {
    const spec = specAt(strings, q);
    if (spec) push(spec, m.index, ['*'], 'reexport');
  });
  return imports;
}

// The class body `{ … }`, capped so a broken file cannot scan forever.
function classBody(masked, from) {
  const open = masked.indexOf('{', from);
  if (open === -1 || open - from > 500) return null;
  let depth = 0;
  const limit = Math.min(masked.length, open + 400_000);
  for (let i = open; i < limit; i++) {
    if (masked[i] === '{') depth++;
    else if (masked[i] === '}' && --depth === 0) return [open, i];
  }
  return null;
}

function collectMethods(masked, starts, className, [open, close]) {
  const decls = [];
  let depth = 1;
  let p = open + 1;
  while (p < close) {
    const nl = masked.indexOf('\n', p);
    const lineEnd = nl === -1 || nl > close ? close : nl;
    const text = masked.slice(p, lineEnd);
    if (depth === 1) {
      const name = (MEMBER_RE.exec(text) ?? MEMBER_ARROW_RE.exec(text))?.[1];
      if (name && !NON_CALL_WORDS.has(name)) {
        decls.push({ name, kind: 'method', line: lineAt(starts, p), exported: false, container: className });
      }
    }
    for (const ch of text) {
      if (ch === '{') depth++;
      else if (ch === '}') depth--;
    }
    p = lineEnd + 1;
  }
  return decls;
}

// `const run = () => …` is a function, `const MAX = 10` a constant.
function looksLikeFunctionValue(masked, afterDecl) {
  const tail = masked.slice(afterDecl, afterDecl + 240);
  const eq = tail.indexOf('=');
  if (eq === -1) return false;
  const rhs = tail.slice(eq + 1);
  if (/^\s*(?:async\s+)?function\b/.test(rhs)) return true;
  return /^\s*(?:async\s*)?(?:\([^)]{0,200}\)|[A-Za-z_$][\w$]*)\s*(?::[^=>]{0,120})?=>/.test(rhs);
}

function collectDecls(masked, starts) {
  const decls = [];
  for (const [re, kind] of [
    [FUNCTION_RE, 'function'],
    [INTERFACE_RE, 'interface'],
    [TYPE_RE, 'type'],
    [ENUM_RE, 'enum'],
  ]) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(masked)) !== null) {
      decls.push({ name: m[2], kind, line: lineAt(starts, m.index), exported: m[1] !== undefined });
    }
  }
  CLASS_RE.lastIndex = 0;
  let cm;
  while ((cm = CLASS_RE.exec(masked)) !== null) {
    decls.push({ name: cm[2], kind: 'class', line: lineAt(starts, cm.index), exported: cm[1] !== undefined });
    const body = classBody(masked, cm.index + cm[0].length);
    if (body) decls.push(...collectMethods(masked, starts, cm[2], body));
  }
  VAR_RE.lastIndex = 0;
  let vm;
  while ((vm = VAR_RE.exec(masked)) !== null) {
    const kind = looksLikeFunctionValue(masked, vm.index + vm[0].length - 1) ? 'function' : 'const';
    decls.push({ name: vm[2], kind, line: lineAt(starts, vm.index), exported: vm[1] !== undefined });
  }
  const exported = new Set();
  EXPORT_BRACE_RE.lastIndex = 0;
  let em;
  while ((em = EXPORT_BRACE_RE.exec(masked)) !== null) {
    if (em[2] === undefined) for (const name of parseExportList(em[1])) exported.add(name);
  }
  for (const d of decls) if (exported.has(d.name)) d.exported = true;
  decls.sort((a, b) => a.line - b.line);
  const partial = decls.length > MAX_DECLS_PER_FILE;
  return { decls: partial ? decls.slice(0, MAX_DECLS_PER_FILE) : decls, partial };
}

// Adds `line` to `refs[name]`; false when a cap dropped it.
function addRef(refs, name, line) {
  const bucket = refs[name];
  if (bucket === undefined) {
    if (Object.keys(refs).length >= MAX_REF_NAMES_PER_FILE) return false;
    refs[name] = [line];
    return true;
  }
  if (bucket[bucket.length - 1] === line) return true;
  if (bucket.length >= MAX_REF_LINES_PER_NAME) return false;
  bucket.push(line);
  return true;
}

function collectRefs(masked, starts, declLines) {
  // Null prototype: a file calling `constructor(` or `toString(` must not hit Object's.
  const refs = Object.create(null);
  let partial = false;
  REF_RE.lastIndex = 0;
  let m;
  while ((m = REF_RE.exec(masked)) !== null) {
    const name = m[1];
    if (name.length < 2 || NON_CALL_WORDS.has(name) || GLOBAL_STOP.has(name)) continue;
    if (m.index > 0 && masked[m.index - 1] === '.' && MEMBER_STOP.has(name)) continue;
    const line = lineAt(starts, m.index);
    // A declaration's own line is not a call site.
    if (declLines.has(`${name}:${line}`)) continue;
    if (!addRef(refs, name, line)) partial = true;
  }
  return { refs, partial };
}

/**
 * Declarations, imports and call sites of one file. `file` only tells a `.vue` SFC
 * (its <script> blocks are parsed, its <template> scanned for component tags).
 */
export function parseSource(text, file) {
  const isVue = file.toLowerCase().endsWith('.vue');
  const starts = lineStarts(text);
  const regions = isVue ? vueScriptRegions(text) : null;
  const { masked, strings } = maskCode(regions ? keepOnly(text, regions) : text);

  const { decls, partial: declsPartial } = collectDecls(masked, starts);
  const declLines = new Set(decls.map((d) => `${d.name}:${d.line}`));
  const { refs, partial: refsPartial } = collectRefs(masked, starts, declLines);
  const imports = collectImports(masked, strings, starts);

  if (regions) {
    const template = keepOnly(text, invertRegions(regions, text.length).filter(([s, e]) => e > s));
    VUE_TAG_RE.lastIndex = 0;
    let m;
    while ((m = VUE_TAG_RE.exec(template)) !== null) addRef(refs, m[1], lineAt(starts, m.index));
  }
  return { decls, imports, refs, partial: declsPartial || refsPartial };
}

// ---- Import resolution -------------------------------------------------------------
//
// Specifier → indexed file: NodeNext `./x.js` → `x.ts`, a folder → its `index`, and the
// `~/` `@/` aliases by a unique path suffix over the indexed files (not tsconfig
// `paths`). An ambiguous suffix resolves to nothing: a wrong edge in a blast radius is
// worse than a missing one.

const EXTS = ['.ts', '.tsx', '.mts', '.cts', '.vue', '.js', '.jsx', '.mjs', '.cjs'];
const JS_TO_TS = { '.js': ['.ts', '.tsx'], '.mjs': ['.mts'], '.cjs': ['.cts'] };
const ALIAS_PREFIXES = ['~~/', '@@/', '~/', '@/'];

function stripExt(p) {
  const dot = p.lastIndexOf('.');
  return dot > p.lastIndexOf('/') ? p.slice(0, dot) : p;
}

function moduleKeys(p) {
  const base = stripExt(p);
  return base.endsWith('/index') ? [base, base.slice(0, -'/index'.length)] : [base];
}

function suffixes(key) {
  const parts = key.split('/');
  return parts.map((_, i) => parts.slice(i).join('/'));
}

/** `{ resolve(fromPath, spec) }` → an indexed path, or null (package, unresolved or ambiguous). */
export function createResolver(paths) {
  const files = new Set(paths);
  const bySuffix = new Map();
  for (const p of paths) {
    for (const key of moduleKeys(p)) {
      for (const s of suffixes(key)) {
        const bucket = bySuffix.get(s);
        if (bucket) bucket.push(p);
        else bySuffix.set(s, [p]);
      }
    }
  }
  const tryBase = (base) => {
    if (files.has(base)) return base;
    const dot = base.lastIndexOf('.');
    if (dot > base.lastIndexOf('/')) {
      for (const ext of JS_TO_TS[base.slice(dot)] ?? []) {
        const candidate = base.slice(0, dot) + ext;
        if (files.has(candidate)) return candidate;
      }
    }
    for (const ext of EXTS) if (files.has(base + ext)) return base + ext;
    for (const ext of EXTS) if (files.has(`${base}/index${ext}`)) return `${base}/index${ext}`;
    return null;
  };
  return {
    resolve(fromPath, spec) {
      if (!spec) return null;
      if (spec.startsWith('./') || spec.startsWith('../') || spec === '.' || spec === '..') {
        const base = path.posix.normalize(`${path.posix.dirname(fromPath)}/${spec}`);
        return tryBase(base.startsWith('./') ? base.slice(2) : base);
      }
      for (const prefix of ALIAS_PREFIXES) {
        if (!spec.startsWith(prefix)) continue;
        const sub = spec.slice(prefix.length);
        const direct = tryBase(sub);
        if (direct) return direct;
        const hits = bySuffix.get(stripExt(sub));
        return hits && hits.length === 1 ? hits[0] : null;
      }
      return null;
    },
  };
}
