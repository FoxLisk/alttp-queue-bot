
CREATE TABLE IF NOT EXISTS category_aliases (
    id              INTEGER PRIMARY KEY NOT NULL,
    game_src_id     TEXT NOT NULL,
    category_src_id TEXT UNIQUE NOT NULL,
    alias           TEXT NOT NULL
);

