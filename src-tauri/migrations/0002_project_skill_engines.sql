-- project_skills: per-engine switches instead of a snapshot of the skill's target.
--
-- A project now records which engines a skill is switched on for, so one skill can be on for
-- Claude in project A and only for Codex in project B. `skills.target` is only the default for a
-- project the skill is newly applied to; changing it leaves the projects that have it alone.
--
-- Each row keeps what it had installed: the old snapshot `target` ('claude' / 'codex' / 'both')
-- becomes the two switches, the hashes and flags are copied as they are. Rows whose project or
-- skill no longer exists are not carried over (foreign keys would cascade them away anyway).
--
-- The table is rebuilt rather than altered so the snapshot column goes away and the column
-- comments below are what the stored schema shows (0001 said `.codex/skills` for Codex; skills
-- have been linked into `.agents/skills` since, `.codex/skills` is only cleaned up).

CREATE TABLE project_skills_new (
    project_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    enabled_claude INTEGER NOT NULL DEFAULT 0,  -- switched on for Claude in this project
    enabled_codex INTEGER NOT NULL DEFAULT 0,   -- switched on for Codex in this project
    synced_hash_claude TEXT,  -- sha256 of the source SKILL.md linked into .claude/skills (NULL: not installed there; a path git tracks is the repo's own and stays NULL even when switched on)
    synced_hash_codex TEXT,   -- sha256 of the source SKILL.md linked into .agents/skills (NULL: same as above)
    applied_at TEXT NOT NULL,
    manual INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, skill_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (skill_id) REFERENCES skills(id) ON DELETE CASCADE
);

INSERT INTO project_skills_new
    (project_id, skill_id, enabled_claude, enabled_codex, synced_hash_claude, synced_hash_codex, applied_at, manual)
SELECT ps.project_id,
       ps.skill_id,
       ps.target IN ('claude', 'both'),
       ps.target IN ('codex', 'both'),
       ps.synced_hash_claude,
       ps.synced_hash_codex,
       ps.applied_at,
       ps.manual
FROM project_skills ps
WHERE EXISTS (SELECT 1 FROM projects p WHERE p.id = ps.project_id)
  AND EXISTS (SELECT 1 FROM skills s WHERE s.id = ps.skill_id);

DROP TABLE project_skills;
ALTER TABLE project_skills_new RENAME TO project_skills;
