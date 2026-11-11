-- The sample response styles (Concise, Explain for a junior, Code review) become rows
-- (UX-36), so the user can edit or delete them like any other style. Until now they lived
-- in code (0011); `default` ("No style": adds nothing) still does.
--
-- `builtin_key` marks a sample: `restore_builtin_styles` puts back the ones deleted, and
-- while a sample's name / description are the seeded English ones the UI shows them
-- translated. A sample's id is its key, so `runs.style`, the `default_style` setting and
-- `projects.default_style` that hold 'concise'… keep pointing at it.
--
-- The texts must match `response_styles::BUILTIN_STYLES` (a test checks it). A global
-- style of the user's that already has a sample's name (e.g. a copy made with the old
-- "Copy to my styles") keeps it: the sample takes "Name (2)", "Name (3)", …
ALTER TABLE response_styles ADD COLUMN builtin_key TEXT;

CREATE UNIQUE INDEX idx_response_styles_builtin_key
    ON response_styles (builtin_key) WHERE builtin_key IS NOT NULL;

WITH RECURSIVE
    n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100),
    seed(key, name, description, body, position) AS (VALUES
        ('concise', 'Concise', 'Answer first, only the needed details, no filler.',
         'Be concise. Lead with the answer in one or two sentences, then only the details that are needed. No preamble, no recap of the question, no closing pleasantries. Prefer short bullet points over paragraphs. Code stays complete and unchanged.',
         -3),
        ('explain_junior', 'Explain for a junior', 'Plain words, terms defined, concrete examples.',
         'Explain for a junior developer. Use short sentences and plain words; when a technical term is unavoidable, define it in one short sentence where it first appears. Prefer a concrete example over theory, and show before / after when something changes. Say why, not only what. Keep technical accuracy and code unchanged.',
         -2),
        ('code_review', 'Code review', 'Verdict first, findings by severity with file and line.',
         'Answer as a careful code reviewer. Start with a one-line verdict. Then list findings ordered by severity (blocking, should fix, nit), each with the file and line, what breaks and in which scenario, and the suggested fix. Separate what you verified from what you assume. No praise or filler.',
         -1)
    )
INSERT OR IGNORE INTO response_styles
    (id, project_id, name, description, body, position, created_at, updated_at, builtin_key)
SELECT
    seed.key,
    NULL,
    (SELECT CASE WHEN n.i = 1 THEN seed.name ELSE seed.name || ' (' || n.i || ')' END
     FROM n
     WHERE NOT EXISTS (
         SELECT 1 FROM response_styles r
         WHERE r.project_id IS NULL
           AND r.name = (CASE WHEN n.i = 1 THEN seed.name ELSE seed.name || ' (' || n.i || ')' END) COLLATE NOCASE)
     ORDER BY n.i LIMIT 1),
    seed.description,
    seed.body,
    seed.position,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    seed.key
FROM seed;
