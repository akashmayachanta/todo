-- Migration 0001: accounts.db, the server's users and their device tokens.
--
-- server/src/auth.rs runs the files in server/migrations in order, and PRAGMA
-- user_version records how many have run. Never edit a migration once it has
-- shipped; add the next numbered file instead.
--
-- Each user's tasks live in their own DB, TASKS_DATA_DIR/user_<id>.db.

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,          -- Argon2id, PHC string
    created_at    TEXT NOT NULL,
    disabled_at   TEXT                    -- set: the user can't sign in, and their devices are refused
);

-- A device signs in once with a username and password and gets a long-lived
-- token for that device. Only a hash of the token is kept.
CREATE TABLE devices (
    id           TEXT PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id),
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,    -- sha256 of the token
    created_at   TEXT NOT NULL,
    last_seen_at TEXT,
    revoked_at   TEXT
);
CREATE INDEX idx_devices_user ON devices (user_id);
