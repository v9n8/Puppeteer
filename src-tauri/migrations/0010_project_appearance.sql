-- How a project shows in the app: its colour, its icon, and whether it is hidden.
--
-- `color` is a design-system tone (`PROJECT_COLORS` in commands/projects.rs), never raw
-- CSS, so it follows light / dark themes. `icon` is a lucide icon name from
-- `PROJECT_ICONS`. NULL keeps the default look (primary tint, folder icon).
--
-- `hidden` only takes the project off the Projects grid, the tab bar and the command
-- palette. Unlike `deleted_at` it changes nothing else: sessions, links, MCP tools and
-- the remote project list keep seeing the project.
ALTER TABLE projects ADD COLUMN color TEXT;
ALTER TABLE projects ADD COLUMN icon TEXT;
ALTER TABLE projects ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
