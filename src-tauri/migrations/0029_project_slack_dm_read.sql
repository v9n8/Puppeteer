-- Reading Slack direct-message threads (#9, RT-27; `slack_threads::dm`). The switch for every
-- project is the `slack_dm_read` setting (off unless turned on). This column can only narrow it
-- for one project: 1 = follow that setting, 0 = never read DMs for this project. Every existing
-- project follows the setting, which is off, so nothing changes until the user turns it on.
ALTER TABLE projects ADD COLUMN slack_dm_read INTEGER NOT NULL DEFAULT 1;
