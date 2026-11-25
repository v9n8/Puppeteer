-- Whether a run's title was set by the app and may still be replaced by a short automatic name
-- (`runs::auto_title`): 1 = app-set (the first prompt cut to a title, or an imported session's
-- fallback), 0 = set by the user, by Focus AI, already replaced, or older than this column.
-- Replaced at most once: the automatic write sets it back to 0.
-- "Imported session" runs got no real name from their transcript when they were imported; marking
-- them lets the session sync name them once.
ALTER TABLE runs ADD COLUMN title_auto INTEGER NOT NULL DEFAULT 0;

UPDATE runs SET title_auto = 1 WHERE title = 'Imported session';
