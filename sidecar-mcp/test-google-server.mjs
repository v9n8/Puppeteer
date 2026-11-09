#!/usr/bin/env node
// Offline contract test for the built-in `google` MCP server.
//
//   node sidecar-mcp/test-google-server.mjs
//
// Needs no Google account: it only drives the JSON-RPC handshake and inspects
// the advertised tool surface. What it guards is exactly what breaks silently
// when the three product groups share one server:
//   - a name collision between groups (Drive and Gmail both define `search` and
//     `delete`, so an unprefixed merge would drop one of them without erroring),
//   - a tool that lost its `account` argument and so can't be pointed at a
//     specific connected account,
//   - anything printed to stdout that isn't a JSON-RPC frame, which corrupts the
//     protocol stream for the host,
//   - a tool call that gets a token anywhere but the credential broker (the
//     server holds no refresh token or client secret).

import { spawn } from 'node:child_process';
import net from 'node:net';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const SERVER = join(dirname(fileURLToPath(import.meta.url)), 'google.mjs');
const GROUPS = { drive_: 15, sheets_: 4, mail_: 15, cal_: 9 };

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  process.stdout.write(`${ok ? '  ok  ' : '  FAIL'} ${label}\n`);
};

/** Send the frames, collect stdout, resolve with parsed frames + raw noise. */
function drive(frames, env = {}) {
  return new Promise((resolve, reject) => {
    // Empty GOOGLE_ACCOUNTS: the server must still advertise its tools; only the
    // handlers need credentials.
    const child = spawn(process.execPath, [SERVER], {
      env: { ...process.env, GOOGLE_ACCOUNTS: '[]', PUPPETEER_BROKER_SOCK: '', ...env },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let out = '';
    let err = '';
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', reject);
    child.on('close', () => {
      const lines = out.split('\n').filter((l) => l.trim());
      const parsed = [];
      const noise = [];
      for (const line of lines) {
        try {
          parsed.push(JSON.parse(line));
        } catch {
          noise.push(line);
        }
      }
      resolve({ parsed, noise, stderr: err });
    });
    for (const f of frames) child.stdin.write(`${JSON.stringify(f)}\n`);
    // Closing stdin right away also checks that calls still in flight answer.
    child.stdin.end();
  });
}

const { parsed, noise, stderr } = await drive([
  { jsonrpc: '2.0', id: 1, method: 'initialize', params: {} },
  { jsonrpc: '2.0', id: 2, method: 'tools/list' },
]);

const init = parsed.find((m) => m.id === 1);
const list = parsed.find((m) => m.id === 2);

check(noise.length === 0, `stdout carries only JSON-RPC frames${noise.length ? ` (saw: ${noise[0].slice(0, 60)})` : ''}`);
check(!!init && init.result.serverInfo.name === 'google', 'initialize reports serverInfo.name = "google"');

const tools = (list && list.result && list.result.tools) || [];
const names = tools.map((t) => t.name);
const expected = Object.values(GROUPS).reduce((a, b) => a + b, 0) + 1; // + list_accounts
check(names.length === expected, `advertises ${expected} tools (got ${names.length})`);

const dupes = names.filter((n, i) => names.indexOf(n) !== i);
check(dupes.length === 0, `no duplicate tool names${dupes.length ? ` (got ${[...new Set(dupes)].join(', ')})` : ''}`);

for (const [prefix, count] of Object.entries(GROUPS)) {
  const got = names.filter((n) => n.startsWith(prefix)).length;
  check(got === count, `group ${prefix}* has ${count} tools (got ${got})`);
}

check(names.includes('list_accounts'), 'list_accounts is present (shared across groups)');

const missingAccount = tools.filter(
  (t) => t.name !== 'list_accounts' && !(t.inputSchema && t.inputSchema.properties && t.inputSchema.properties.account),
);
check(missingAccount.length === 0, `every tool takes an \`account\` argument${missingAccount.length ? ` (missing on ${missingAccount.map((t) => t.name).join(', ')})` : ''}`);

const undescribed = tools.filter((t) => !t.description || t.description.length < 20);
check(undescribed.length === 0, `every tool has a usable description${undescribed.length ? ` (thin: ${undescribed.map((t) => t.name).join(', ')})` : ''}`);

// The whole point of the merge is that these stayed distinguishable.
for (const n of ['drive_search', 'mail_search', 'drive_delete', 'mail_delete', 'cal_delete_event']) {
  check(names.includes(n), `collision-prone tool kept its own name: ${n}`);
}

// Calendar writes must default to NOT emailing attendees (see calendar.mjs).
for (const n of ['cal_create_event', 'cal_update_event', 'cal_delete_event', 'cal_respond_event']) {
  const t = tools.find((x) => x.name === n);
  const notify = t && t.inputSchema.properties.notify;
  check(!!notify && /defaults to false/i.test(notify.description || ''), `${n} documents notify=false as the default`);
}

// With an account but no broker, a call fails closed with a broker error.
{
  const { parsed: frames } = await drive(
    [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: {} },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'drive_list', arguments: {} } },
    ],
    { GOOGLE_ACCOUNTS: JSON.stringify([{ label: 'A', email: 'a@x', default: true }]) },
  );
  const r = frames.find((m) => m.id === 2);
  const text = (r && r.result && r.result.content && r.result.content[0].text) || '';
  check(!!r && r.result.isError && /broker/i.test(text), `a call without the broker fails closed (got: ${text.slice(0, 80)})`);
}

// Tools that touch local files only accept paths inside the project or
// ~/Downloads (symlinks followed), never HOME dot-folders or ~/Library. A refused
// path fails before any Google call; an accepted one gets as far as the broker.
{
  const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'google-paths-')));
  const home = join(tmp, 'home');
  const proj = join(tmp, 'proj');
  const outside = join(tmp, 'outside');
  for (const d of [join(home, 'Downloads'), join(home, '.ssh'), join(home, 'Library', 'LaunchAgents'), proj, outside]) {
    mkdirSync(d, { recursive: true });
  }
  writeFileSync(join(home, '.ssh', 'id_ed25519'), 'secret');
  writeFileSync(join(proj, 'notes.txt'), 'hi');
  symlinkSync(outside, join(proj, 'link'));
  symlinkSync(join(outside, 'planted.txt'), join(proj, 'dangling'));

  const cases = [
    ['drive_download', { file_id: 'f', dest_path: join(outside, 'x.txt') }, false, 'download outside the project'],
    ['drive_download', { file_id: 'f', dest_path: join(`${proj}-evil`, 'x.txt') }, false, 'download into a sibling sharing the root prefix'],
    ['drive_download', { file_id: 'f', dest_path: `${proj}/../outside/x.txt` }, false, 'download escaping with ..'],
    ['drive_download', { file_id: 'f', dest_path: join(proj, 'link', 'x.txt') }, false, 'download through a symlink out of the project'],
    ['drive_download', { file_id: 'f', dest_path: join(proj, 'dangling') }, false, 'download onto a dangling symlink'],
    ['drive_download', { file_id: 'f', dest_path: join(home, '.ssh', 'authorized_keys') }, false, 'download into ~/.ssh'],
    ['drive_upload', { local_path: join(home, '.ssh', 'id_ed25519') }, false, 'upload of ~/.ssh/id_ed25519'],
    ['mail_download_attachment', { message_id: 'm', dest_path: join(home, 'Library', 'LaunchAgents', 'x.plist') }, false, 'attachment into ~/Library/LaunchAgents'],
    ['mail_download_attachments', { message_id: 'm', dest_dir: join(outside, 'made', 'sub') }, false, 'attachments into a new dir outside'],
    ['drive_download', { file_id: 'f', dest_path: join(proj, 'sub', 'x.txt') }, true, 'download inside the project'],
    ['drive_download', { file_id: 'f', dest_path: join(home, 'Downloads', 'x.pdf') }, true, 'download into ~/Downloads'],
    ['drive_upload', { local_path: join(proj, 'notes.txt') }, true, 'upload of a project file'],
    ['mail_download_attachments', { message_id: 'm', dest_dir: join(proj, 'attachments') }, true, 'attachments into the project'],
  ];
  const { parsed: frames } = await drive(
    [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: {} },
      ...cases.map(([name, args], i) => ({ jsonrpc: '2.0', id: 10 + i, method: 'tools/call', params: { name, arguments: args } })),
    ],
    {
      // os.homedir() reads USERPROFILE on Windows.
      HOME: home,
      USERPROFILE: home,
      PUPPETEER_PROJECT_PATH: proj,
      GOOGLE_ACCOUNTS: JSON.stringify([{ label: 'A', email: 'a@x', default: true }]),
    },
  );
  cases.forEach(([, , allowed, label], i) => {
    const r = frames.find((m) => m.id === 10 + i);
    const text = (r && r.result && r.result.content && r.result.content[0].text) || '';
    const reachedBroker = /broker/i.test(text);
    check(allowed ? reachedBroker : !reachedBroker && /refus/i.test(text), `${allowed ? 'accepts' : 'refuses'} ${label} (got: ${text.slice(0, 80)})`);
  });
  // Even with HOME itself as the project, its dot-folders and ~/Library stay off limits.
  const homeCases = [
    [join(home, '.ssh', 'authorized_keys'), /~\/\.ssh/],
    [join(home, 'LIBRARY', 'LaunchAgents', 'x.plist'), /~\/library/i],
    [join(home, 'Documents', 'x.txt'), /broker/i],
  ];
  const { parsed: homeFrames } = await drive(
    [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: {} },
      ...homeCases.map(([dest], i) => ({ jsonrpc: '2.0', id: 10 + i, method: 'tools/call', params: { name: 'drive_download', arguments: { file_id: 'f', dest_path: dest } } })),
    ],
    { HOME: home, USERPROFILE: home, PUPPETEER_PROJECT_PATH: home, GOOGLE_ACCOUNTS: JSON.stringify([{ label: 'A', email: 'a@x', default: true }]) },
  );
  homeCases.forEach(([dest, want], i) => {
    const r = homeFrames.find((m) => m.id === 10 + i);
    const text = (r && r.result && r.result.content && r.result.content[0].text) || '';
    check(want.test(text), `project = HOME: ${dest.slice(home.length)} → ${want} (got: ${text.slice(0, 80)})`);
  });
  check(!existsSync(join(outside, 'made')), 'a refused dest_dir is not created');
  check(!existsSync(join(outside, 'planted.txt')), 'nothing written through the dangling symlink');
  rmSync(tmp, { recursive: true, force: true });
}

/** A fake credential broker that answers `reply` after `delayMs`. */
async function fakeBroker(dir, reply, delayMs = 0) {
  const sock = process.platform === 'win32' ? `\\\\.\\pipe\\puppeteer-google-${process.pid}-${Date.now()}` : join(dir, 'broker.sock');
  const server = net.createServer((conn) => {
    conn.on('data', () => setTimeout(() => conn.end(`${JSON.stringify(reply)}\n`), delayMs));
  });
  await new Promise((res) => server.listen(sock, res));
  return { sock, close: () => server.close() };
}

// The host closes stdin when the run ends; a call still waiting on the broker
// must answer before the server exits, not be dropped.
{
  const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'google-eof-')));
  const broker = await fakeBroker(tmp, { decision: 'deny', reason: 'slow broker' }, 300);
  const child = spawn(process.execPath, [SERVER], {
    env: {
      ...process.env,
      GOOGLE_ACCOUNTS: JSON.stringify([{ label: 'A', email: 'a@x', default: true }]),
      PUPPETEER_BROKER_SOCK: broker.sock,
      PUPPETEER_PROJECT_ID: 'p',
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  let out = '';
  child.stdout.on('data', (d) => (out += d));
  child.stdin.end(`${JSON.stringify({ jsonrpc: '2.0', id: 7, method: 'tools/call', params: { name: 'drive_list', arguments: {} } })}\n`);
  await new Promise((res) => child.on('close', res));
  broker.close();
  const r = out.split('\n').filter(Boolean).map((l) => JSON.parse(l)).find((m) => m.id === 7);
  const text = (r && r.result && r.result.content && r.result.content[0].text) || '';
  check(/slow broker/.test(text), `a call in flight when stdin closes still answers (got: ${text.slice(0, 80) || 'no reply'})`);
  rmSync(tmp, { recursive: true, force: true });
}

// Ids from the model (or a mail it read) go into the URL path: one carrying
// `/../` must stay a single encoded segment, never re-target another endpoint.
{
  const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'google-ids-')));
  writeFileSync(join(tmp, 'up.txt'), 'hi');
  const broker = await fakeBroker(tmp, { decision: 'allow', token: 't' });
  // lib/google.mjs reads GOOGLE_ACCOUNTS when first imported, just below.
  Object.assign(process.env, {
    GOOGLE_ACCOUNTS: JSON.stringify([{ label: 'A', email: 'a@x', default: true }]),
    PUPPETEER_BROKER_SOCK: broker.sock,
    PUPPETEER_PROJECT_ID: 'p',
    PUPPETEER_PROJECT_PATH: tmp,
  });
  const EVIL = 'ID/../x';
  const body = {
    id: EVIL,
    threadId: EVIL,
    name: 'n',
    mimeType: 'text/plain',
    parents: [EVIL],
    messages: [{ id: EVIL }],
    payload: { headers: [], parts: [{ partId: '1', filename: 'a.txt', mimeType: 'text/plain', body: { attachmentId: EVIL, size: 1 } }] },
    data: 'aGk',
  };
  const urls = [];
  let current = '';
  const realFetch = globalThis.fetch;
  globalThis.fetch = async (url) => {
    urls.push({ tool: current, url: String(url) });
    return new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });
  };
  const groups = {
    drive_: (await import('./lib/google/drive.mjs')).tools,
    mail_: (await import('./lib/google/mail.mjs')).tools,
  };
  const args = {
    file_id: EVIL, message_id: EVIL, thread_id: EVIL, permission_id: EVIL, attachment_id: EVIL, new_parent_id: 'p',
    name: 'n', content: 'c', role: 'reader', type: 'anyone', email: 'a@x', to: 'a@x', subject: 's', body: 'b',
    add_label_ids: ['L'], read: true, local_path: join(tmp, 'up.txt'), dest_path: join(tmp, 'out.txt'), dest_dir: join(tmp, 'out'),
  };
  for (const [prefix, tools] of Object.entries(groups)) {
    for (const [name, tool] of Object.entries(tools)) {
      current = `${prefix}${name}`;
      await Promise.resolve().then(() => tool.handler({ ...args })).catch(() => {});
    }
  }
  globalThis.fetch = realFetch;
  broker.close();
  const raw = urls.filter((u) => u.url.split('?')[0].includes(EVIL));
  check(raw.length === 0, `ids are encoded in URL paths${raw.length ? ` (raw in: ${[...new Set(raw.map((u) => u.tool))].join(', ')})` : ''}`);
  check(urls.some((u) => u.url.includes(encodeURIComponent(EVIL))), 'the id still reaches the URL, encoded');
  rmSync(tmp, { recursive: true, force: true });
}

if (failures.length) {
  process.stderr.write(`\n${failures.length} check(s) failed.\n`);
  if (stderr.trim()) process.stderr.write(`server stderr:\n${stderr}\n`);
  process.exit(1);
}
process.stdout.write(`\nAll checks passed (${names.length} tools).\n`);
