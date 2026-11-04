-- Migration 0001: the initial tasks schema.
--
-- Tasks DB: one per user on the server (TASKS_DATA_DIR/user_<id>.db) and one
-- per device in the apps (tasks.db in the app data folder). Same schema on both.
-- tasks-core (core/src/db.rs) runs the files in core/migrations in order, and
-- PRAGMA user_version records how many have run. Never edit a migration once it
-- has shipped; add the next numbered file instead.
--
-- Every synced table has these columns:
--   id          TEXT, a UUIDv7 made on the device that created the row, so
--               offline creates never collide.
--   created_at  TEXT, an ISO instant (UTC, "2026-11-04T09:30:00.000Z").
--   updated_at  INTEGER, milliseconds since the epoch. With updated_by it orders
--               the versions of a row: the later (updated_at, updated_by) wins.
--   updated_by  TEXT, the device id that made this version ("server" for an
--               edit made through the REST API).
--   deleted_at  INTEGER (ms), set on a deleted row (a tombstone). Rows are never
--               removed, so a delete reaches every device. Cascades are written
--               by tasks-core as tombstones, not by ON DELETE CASCADE.
--   server_seq  INTEGER, the server's change counter when it accepted this
--               version; a device pulls the rows above the last one it saw.
--   dirty       INTEGER, device only: 1 for a change not pushed yet.
--
-- No foreign keys: rows arrive from other devices in any order. tasks-core
-- repairs orphans (a task whose project is gone, etc.) after every merge.
--
-- due_date / due_time are the user's local date and time as typed; remind_at
-- and completed_at are ISO instants (UTC). recurrence is the phrase read by
-- app/src/lib/taskParse.ts. task_filters.criteria and .fields are JSON text.
-- task_completions logs every tick for the Karma view; an untick is a tombstone
-- on the task's latest completion.

CREATE TABLE task_projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT,                 -- palette key: grey, red, orange, … pink
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT 0,
    updated_by  TEXT NOT NULL DEFAULT '',
    deleted_at  INTEGER,
    server_seq  INTEGER,
    dirty       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE task_categories (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL,
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT 0,
    updated_by  TEXT NOT NULL DEFAULT '',
    deleted_at  INTEGER,
    server_seq  INTEGER,
    dirty       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_task_categories_project ON task_categories (project_id);

CREATE TABLE tasks (
    id           TEXT PRIMARY KEY,
    project_id   TEXT,                -- NULL = Inbox
    category_id  TEXT,
    parent_id    TEXT,                -- set on a subtask (one level deep)
    title        TEXT NOT NULL,
    description  TEXT,
    priority     INTEGER NOT NULL DEFAULT 4,  -- 1 (highest) … 4
    due_date     TEXT,                -- YYYY-MM-DD, local
    due_time     TEXT,                -- HH:MM, local
    duration_minutes INTEGER,
    location     TEXT,
    remind_at    TEXT,                -- ISO instant, UTC
    recurrence   TEXT,                -- e.g. 'every monday'
    position     INTEGER NOT NULL DEFAULT 0,
    completed_at TEXT,                -- ISO instant, UTC
    created_at   TEXT NOT NULL,
    updated_at   INTEGER NOT NULL DEFAULT 0,
    updated_by   TEXT NOT NULL DEFAULT '',
    deleted_at   INTEGER,
    server_seq   INTEGER,
    dirty        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tasks_project_category ON tasks (project_id, category_id);
CREATE INDEX idx_tasks_due    ON tasks (due_date);
CREATE INDEX idx_tasks_parent ON tasks (parent_id);
CREATE INDEX idx_tasks_remind ON tasks (remind_at);

CREATE TABLE task_filters (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    color       TEXT,
    criteria    TEXT NOT NULL,        -- JSON {projects: [id|null], categories: [id], priorities: [1-4], due: [...]}
    fields      TEXT NOT NULL,        -- JSON list of row details shown
    group_by    TEXT NOT NULL DEFAULT 'none',
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT 0,
    updated_by  TEXT NOT NULL DEFAULT '',
    deleted_at  INTEGER,
    server_seq  INTEGER,
    dirty       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE task_completions (
    id           TEXT PRIMARY KEY,
    task_id      TEXT,                -- shown as null once the task is deleted
    completed_at TEXT NOT NULL,       -- ISO instant, UTC
    created_at   TEXT NOT NULL,
    updated_at   INTEGER NOT NULL DEFAULT 0,
    updated_by   TEXT NOT NULL DEFAULT '',
    deleted_at   INTEGER,             -- an untick
    server_seq   INTEGER,
    dirty        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_task_completions_at   ON task_completions (completed_at);
CREATE INDEX idx_task_completions_task ON task_completions (task_id);

CREATE INDEX idx_task_projects_seq    ON task_projects (server_seq);
CREATE INDEX idx_task_categories_seq  ON task_categories (server_seq);
CREATE INDEX idx_tasks_seq            ON tasks (server_seq);
CREATE INDEX idx_task_filters_seq     ON task_filters (server_seq);
CREATE INDEX idx_task_completions_seq ON task_completions (server_seq);

-- Key/value state. Server: seq (the change counter). Device: device_id,
-- last_seq (the highest server_seq pulled).
CREATE TABLE sync_state (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);
