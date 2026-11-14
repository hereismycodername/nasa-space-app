CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    astronaut_id TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('simulator', 'manual', 'sensor'))
);
INSERT INTO devices VALUES ('simulator-01', 'demo-astronaut', 'simulator');
INSERT INTO devices VALUES ('manual-01', 'demo-astronaut', 'manual');
CREATE TABLE measurements (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL REFERENCES devices(id),
    astronaut_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    metric TEXT NOT NULL,
    value REAL NOT NULL,
    unit TEXT NOT NULL,
    measured_at TEXT,
    received_at TEXT NOT NULL,
    source TEXT NOT NULL,
    UNIQUE(device_id, session_id, sequence)
);
CREATE INDEX measurements_history ON measurements(astronaut_id, received_at);
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    completed_at TEXT
);
INSERT INTO tasks VALUES ('daily-check', 'Заполнить ежедневные измерения', NULL);
