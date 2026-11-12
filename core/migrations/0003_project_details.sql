-- Migration 0003: favourite projects and descriptions for projects and categories.
--
--   task_projects.favorite      INTEGER, 1 = listed under Favorites at the top of
--                               the sidebar; NULL or 0 = not.
--   task_projects.description   TEXT, shown under the project's title; NULL = none.
--   task_categories.description TEXT, shown under the category's heading; NULL = none.
--
-- All nullable on purpose: a device on an older build pushes rows without these
-- columns, and the merge writes a missing column as NULL. Descriptions are stored
-- as typed, links included, like task descriptions.

ALTER TABLE task_projects ADD COLUMN favorite INTEGER;
ALTER TABLE task_projects ADD COLUMN description TEXT;
ALTER TABLE task_categories ADD COLUMN description TEXT;
