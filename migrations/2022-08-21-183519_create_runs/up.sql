-- Your SQL goes here
CREATE TABLE IF NOT EXISTS runs (
    id         INTEGER PRIMARY KEY NOT NULL,
    submitted  TEXT NULL,
    thread_id  TEXT NULL,
    run_id     TEXT NOT NULL UNIQUE
)