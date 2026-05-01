-- zero-session V001 — initial schema.
--
-- Design goals (spec v2.1 §9):
--   * Persistent:   one database at `~/.zero/state.db`, WAL-journalled.
--   * Resumable:    the most recent session can be re-rendered on
--                   launch by replaying its events in order.
--   * Forkable:     a session references a `parent_ulid` so operators
--                   can branch a decision tree without losing the
--                   original context.
--   * Replayable:   `(session_id, seq)` is a monotonic ordering; no
--                   wall-clock shuffling.
--   * Shareable:    `ulid` is the opaque external id — file export
--                   and re-import uses it, never the local `id`.
--
-- Do NOT add operator-state events here (see ADR-016 — those live
-- on the engine host under /operator/events). This table is CLI-
-- local: just the conversation render log and session index.

CREATE TABLE sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    ulid            TEXT    NOT NULL UNIQUE,
    started_at      TEXT    NOT NULL,
    ended_at        TEXT,
    engine_base_url TEXT,
    cli_version     TEXT    NOT NULL,
    parent_ulid     TEXT
);

CREATE INDEX sessions_started_at_idx ON sessions (started_at DESC);

CREATE TABLE events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    at         TEXT    NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN (
                    'prompt','system','command','warn','alert','mode_change'
               )),
    text       TEXT    NOT NULL,
    UNIQUE (session_id, seq)
);

CREATE INDEX events_session_seq_idx ON events (session_id, seq);

-- Journey milestones — flags the TUI reads on launch to decide
-- whether to run onboarding, the first-live-trade ceremony, etc.
-- One row per key; newest wins.
CREATE TABLE milestones (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    at    TEXT NOT NULL
);
