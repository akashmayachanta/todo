-- Migration 0002: the account's profile (display name and photo), so it follows
-- the user to every device.
--
-- One row per tasks DB (so one per account), always with id 'me'; the newest
-- version wins like any other row. Same sync columns as 0001.
--   display_name  TEXT, shown instead of the username; NULL = the username.
--   photo         TEXT, a small square JPEG as a data URL ("data:image/jpeg;base64,…");
--                 NULL = no photo. tasks-core caps its size.

CREATE TABLE profile (
    id            TEXT PRIMARY KEY,
    display_name  TEXT,
    photo         TEXT,
    created_at    TEXT NOT NULL,
    updated_at    INTEGER NOT NULL DEFAULT 0,
    updated_by    TEXT NOT NULL DEFAULT '',
    deleted_at    INTEGER,
    server_seq    INTEGER,
    dirty         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_profile_seq ON profile (server_seq);
