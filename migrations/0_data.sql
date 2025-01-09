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

-- New table for detailed request logging
CREATE TABLE IF NOT EXISTS request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    endpoint TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER DEFAULT 0,
    status_code INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    error_message TEXT,
    request_id TEXT NOT NULL,
    
    -- Usage statistics fields
    queue_time_ms INTEGER,
    prompt_time_ms INTEGER,
    completion_time_ms INTEGER,
    total_time_ms INTEGER,
    total_tokens INTEGER DEFAULT 0,
    
    FOREIGN KEY (uid) REFERENCES hits(uid)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_tokens_uid_date ON tokens(uid, date);
CREATE INDEX IF NOT EXISTS idx_request_logs_uid ON request_logs(uid);
CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_request_logs_model ON request_logs(model);
