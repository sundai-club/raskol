CREATE TABLE IF NOT EXISTS hits (
    uid TEXT PRIMARY KEY,
    count_of_all INTEGER NOT NULL,
    time_of_last INTEGER NOT NULL
)
