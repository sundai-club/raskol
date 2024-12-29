CREATE TABLE IF NOT EXISTS hits (
    uid TEXT PRIMARY KEY,
    count_of_all INTEGER NOT NULL,
    time_of_last INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tokens (
    uid TEXT NOT NULL,
    date TEXT NOT NULL,
    total INTEGER NOT NULL,

    UNIQUE (uid, date)
);

CREATE INDEX IF NOT EXISTS idx_tokens_uid_date ON tokens(uid, date);
