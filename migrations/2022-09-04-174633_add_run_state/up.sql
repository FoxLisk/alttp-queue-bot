-- it appears that this runs in a transaction on its own, although i'm hard pressed to find a straight assertion of this
-- in the docs
CREATE TABLE __new_runs (
    id         INTEGER PRIMARY KEY NOT NULL,
    submitted  TEXT NULL,
    thread_id  TEXT NULL,
    state      TEXT NOT NULL,
    run_id     TEXT NOT NULL UNIQUE,
    src_state  TEXT NOT NULL
);

INSERT INTO __new_runs (id, submitted, thread_id, state, run_id, src_state)
SELECT                  id, submitted, thread_id, state, run_id, 'New'
FROM runs;

DROP TABLE runs;

ALTER TABLE __new_runs RENAME TO runs;

