-- AstroCheck interface additions (not part of the team backend): crew roster, acknowledged findings,
-- uplink/crew messages and small key-value settings (mission clock, environment, link state).
CREATE TABLE crew (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    sort INTEGER NOT NULL
);
CREATE TABLE alert_acks (
    crew_id TEXT NOT NULL REFERENCES crew(id),
    indicator TEXT NOT NULL,
    acked_at TEXT NOT NULL,
    PRIMARY KEY (crew_id, indicator)
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crew_id TEXT NOT NULL REFERENCES crew(id),
    from_ground INTEGER NOT NULL CHECK (from_ground IN (0, 1)),
    text TEXT NOT NULL,
    day INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX messages_by_crew ON messages(crew_id, id);
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
