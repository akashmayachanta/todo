-- Migration 0004: categories are now called sections, everywhere.
--   task_categories -> task_sections
--   tasks.category_id -> tasks.section_id
--   task_filters.criteria: the "categories" key -> "sections"
--   task_filters.group_by: 'category' -> 'section'
-- Every device and the server run this on their own copy, so the rows keep
-- their updated_at and dirty flags; nothing new needs to sync.

ALTER TABLE task_categories RENAME TO task_sections;
ALTER TABLE tasks RENAME COLUMN category_id TO section_id;

DROP INDEX IF EXISTS idx_task_categories_project;
DROP INDEX IF EXISTS idx_task_categories_seq;
DROP INDEX IF EXISTS idx_tasks_project_category;
CREATE INDEX idx_task_sections_project ON task_sections (project_id);
CREATE INDEX idx_task_sections_seq     ON task_sections (server_seq);
CREATE INDEX idx_tasks_project_section ON tasks (project_id, section_id);

UPDATE task_filters
SET criteria = json_set(json_remove(criteria, '$.categories'), '$.sections', json(json_extract(criteria, '$.categories')))
WHERE json_valid(criteria) AND json_type(criteria, '$.categories') IS NOT NULL;

UPDATE task_filters SET group_by = 'section' WHERE group_by = 'category';
