-- New default session flags (UI review v3, UR-04): a work-status set replaces the four
-- of 0023. A clean reset, as decided while nobody uses the flags yet: every flag goes,
-- shared and the projects' own, so 0023's delete trigger sets every session back to
-- "No flag" and drops the hidden marks. 0023 itself stays as it ran (sqlx checks the
-- checksum of an applied migration).
--
-- The seed must match `session_flags::DEFAULT_FLAGS` (a test checks it).
DELETE FROM session_flags;

INSERT INTO session_flags (id, project_id, name, color, position, builtin_key, created_at, updated_at) VALUES
    ('backlog', NULL, 'Backlog', 'neutral', 0, 'backlog',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('todo', NULL, 'To Do', 'primary', 1, 'todo',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('in_progress', NULL, 'In Progress', 'info', 2, 'in_progress',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('in_review', NULL, 'In Review', 'warning', 3, 'in_review',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('done', NULL, 'Done', 'success', 4, 'done',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
