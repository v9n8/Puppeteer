// `schedule_propose`: the agent drafts a schedule for the current project
// (migration 0007, src-tauri/src/schedules). It is a PROPOSAL only: written off
// (`enabled = 0`), marked `proposed = 1`, read-only `plan` mode, so the app
// never fires it until the user opens it in Schedules, saves it and switches it
// on (switching on is local-only). A scheduled run has nobody watching it, so an
// agent never starts one (AWOG schedule-propose-tool.ts).
//
// Hidden while `orchestration.schedules` is off (group `schedules`,
// lib/feature-groups.mjs). `timing` is the JSON `schedules::Timing` reads.

import { randomUUID } from 'node:crypto';
import { db, requireProject, nowRfc3339 } from './store.mjs';

// Unreviewed proposals waiting at once, app-wide: past this they are noise.
export const MAX_PENDING_PROPOSALS = 10;
const MAX_INTERVAL_MINUTES = 7 * 24 * 60;
const TIME_RE = /^([01]\d|2[0-3]):[0-5]\d$/;
const ONCE_RE = /^\d{4}-\d{2}-\d{2}T([01]\d|2[0-3]):[0-5]\d$/;

/** The `timing` of a proposal from the tool's arguments; throws on a bad mix. */
export function timingOf({ every_minutes, at, weekdays, once_at } = {}) {
  const given = [every_minutes != null, at != null, once_at != null].filter(Boolean).length;
  if (given !== 1) throw new Error('Give exactly one timing: `every_minutes`, `at` (with optional `weekdays`), or `once_at`.');
  if (weekdays != null && at == null) throw new Error('`weekdays` goes with `at`.');
  if (every_minutes != null) {
    const n = Number(every_minutes);
    if (!Number.isInteger(n) || n < 1 || n > MAX_INTERVAL_MINUTES) {
      throw new Error(`\`every_minutes\` must be a whole number 1–${MAX_INTERVAL_MINUTES}.`);
    }
    return { kind: 'interval', every_minutes: n };
  }
  if (once_at != null) {
    const s = String(once_at).trim();
    if (!ONCE_RE.test(s) || Number.isNaN(Date.parse(s))) throw new Error('`once_at` must be YYYY-MM-DDTHH:MM (local time).');
    return { kind: 'once', at: s };
  }
  const time = String(at).trim();
  if (!TIME_RE.test(time)) throw new Error('`at` must be HH:MM (24 h, local time).');
  if (weekdays == null || (Array.isArray(weekdays) && weekdays.length === 0)) return { kind: 'daily', time };
  const days = Array.isArray(weekdays) ? weekdays.map(Number) : [NaN];
  if (days.some((d) => !Number.isInteger(d) || d < 0 || d > 6)) throw new Error('`weekdays` are 0 (Sunday) … 6 (Saturday).');
  return { kind: 'weekly', weekdays: [...new Set(days)].sort((a, b) => a - b), time };
}

/** Insert the proposal; returns its row. */
export function proposeSchedule(args = {}) {
  const projectId = requireProject();
  const name = String(args.name ?? '').trim();
  const prompt = String(args.prompt ?? '').trim();
  if (!name || name.length > 200) throw new Error('`name` is required (at most 200 characters).');
  if (!prompt || prompt.length > 20000) throw new Error('`prompt` is required (at most 20000 characters).');
  const timing = timingOf(args);
  const d = db();
  // Proposals of a project in the trash are hidden in Schedules, so they do not hold a slot.
  const pending = d
    .prepare(
      'SELECT COUNT(*) AS n FROM schedules WHERE proposed = 1 AND project_id IN (SELECT id FROM projects WHERE deleted_at IS NULL)',
    )
    .get().n;
  if (pending >= MAX_PENDING_PROPOSALS) {
    throw new Error(`${pending} proposed schedules are already waiting for review; ask the user to review or delete them first.`);
  }
  const id = randomUUID();
  const now = nowRfc3339();
  d.prepare(
    `INSERT INTO schedules (id, project_id, name, prompt, engine, permission_mode, timing, enabled, proposed, created_at, updated_at)
     VALUES (?, ?, ?, ?, NULL, 'plan', ?, 0, 1, ?, ?)`,
  ).run(id, projectId, name, prompt, JSON.stringify(timing), now, now);
  return d.prepare('SELECT * FROM schedules WHERE id = ?').get(id);
}

export function scheduleTools() {
  return {
    schedule_propose: {
      description:
        'Propose a schedule in the Puppeteer app when the user asks for something to happen on a timer (every N minutes, daily, on some weekdays, or once). ' +
        'The schedule is created OFF: nothing runs until the user reviews it in Puppeteer → Schedules, saves it and switches it on. ' +
        'Each run opens a new session in the current project and sends it `prompt`, with the default engine, in read-only plan mode unless the user changes it. ' +
        'Timing — give exactly one: `every_minutes`, or `at` (daily at HH:MM local time), or `at` + `weekdays`, or `once_at`. ' +
        `Schedules only run while the Puppeteer app is open. At most ${MAX_PENDING_PROPOSALS} proposals wait for review at once. ` +
        'After calling it, tell the user to review and switch it on in Schedules.',
      inputSchema: {
        type: 'object',
        properties: {
          name: { type: 'string', description: 'Short name the user will recognise in the Schedules list.' },
          prompt: { type: 'string', description: 'The prompt sent to a new session at each run.' },
          every_minutes: { type: 'integer', minimum: 1, maximum: MAX_INTERVAL_MINUTES, description: 'Run every N minutes.' },
          at: { type: 'string', description: 'Local time of day "HH:MM" (24 h). Alone = daily; with `weekdays` = those days only.' },
          weekdays: { type: 'array', items: { type: 'integer', minimum: 0, maximum: 6 }, description: 'Days for `at`: 0 = Sunday … 6 = Saturday.' },
          once_at: { type: 'string', description: 'One run at this local date and time, "YYYY-MM-DDTHH:MM".' },
        },
        required: ['name', 'prompt'],
      },
      handler: (a) => {
        const s = proposeSchedule(a);
        return (
          `Proposed schedule "${s.name}" (id ${s.id}). It is OFF and will not run until the user reviews it in ` +
          'Puppeteer → Schedules, saves it and switches it on. Tell the user that, and that schedules only run while the app is open.'
        );
      },
    },
  };
}
