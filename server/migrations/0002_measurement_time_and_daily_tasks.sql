-- Keep the original timestamp text and message identity unchanged.
-- INTEGER UTC milliseconds truncate sub-millisecond digits, matching Chrono.
-- Parse whole seconds separately: SQLite's datetime parser rounds fractions
-- and does not accept all RFC3339 offsets or leap seconds accepted by Chrono.
-- VIRTUAL columns backfill existing rows and derive the same values on every write.
ALTER TABLE measurements ADD COLUMN measured_at_ms INTEGER GENERATED ALWAYS AS (
    CASE WHEN
        substr(measured_at, 1, 19) GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9][Tt ][0-9][0-9]:[0-9][0-9]:[0-9][0-9]'
        AND date(substr(measured_at, 1, 10), '+0 days') = substr(measured_at, 1, 10)
        AND CAST(substr(measured_at, 12, 2) AS INTEGER) BETWEEN 0 AND 23
        AND CAST(substr(measured_at, 15, 2) AS INTEGER) BETWEEN 0 AND 59
        AND CAST(substr(measured_at, 18, 2) AS INTEGER) BETWEEN 0 AND 60
        AND (upper(substr(measured_at, -1)) = 'Z' OR (
            substr(measured_at, -6) GLOB '[+−-][0-9][0-9]:[0-9][0-9]'
            AND CAST(substr(measured_at, -5, 2) AS INTEGER) BETWEEN 0 AND 23
            AND CAST(substr(measured_at, -2) AS INTEGER) BETWEEN 0 AND 59
        ))
        AND (
            length(measured_at) = 19 + CASE WHEN upper(substr(measured_at, -1)) = 'Z' THEN 1 ELSE 6 END
            OR (
                substr(measured_at, 20, 1) = '.'
                AND length(measured_at) > 20 + CASE WHEN upper(substr(measured_at, -1)) = 'Z' THEN 1 ELSE 6 END
                AND substr(measured_at, 21, length(measured_at) - 20 - CASE WHEN upper(substr(measured_at, -1)) = 'Z' THEN 1 ELSE 6 END) NOT GLOB '*[^0-9]*'
            )
        )
    THEN
        unixepoch(substr(measured_at, 1, 10) || 'T' || substr(measured_at, 12, 6)
            || CASE WHEN substr(measured_at, 18, 2) = '60' THEN '59' ELSE substr(measured_at, 18, 2) END || 'Z') * 1000
        + CASE WHEN substr(measured_at, 18, 2) = '60' THEN 1000 ELSE 0 END
        + CASE WHEN substr(measured_at, 20, 1) = '.'
            THEN CAST(substr(substr(measured_at, 21, length(measured_at) - 20
                - CASE WHEN upper(substr(measured_at, -1)) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
            ELSE 0 END
        - CASE WHEN upper(substr(measured_at, -1)) = 'Z' THEN 0 ELSE
            (CASE substr(measured_at, -6, 1) WHEN '+' THEN 1 ELSE -1 END)
            * (CAST(substr(measured_at, -5, 2) AS INTEGER) * 60 + CAST(substr(measured_at, -2) AS INTEGER)) * 60000 END
    END
) VIRTUAL CHECK (measured_at IS NULL OR measured_at_ms IS NOT NULL);

ALTER TABLE measurements ADD COLUMN received_at_ms INTEGER GENERATED ALWAYS AS (
    CASE WHEN
        substr(received_at, 1, 19) GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9][Tt ][0-9][0-9]:[0-9][0-9]:[0-9][0-9]'
        AND date(substr(received_at, 1, 10), '+0 days') = substr(received_at, 1, 10)
        AND CAST(substr(received_at, 12, 2) AS INTEGER) BETWEEN 0 AND 23
        AND CAST(substr(received_at, 15, 2) AS INTEGER) BETWEEN 0 AND 59
        AND CAST(substr(received_at, 18, 2) AS INTEGER) BETWEEN 0 AND 60
        AND (upper(substr(received_at, -1)) = 'Z' OR (
            substr(received_at, -6) GLOB '[+−-][0-9][0-9]:[0-9][0-9]'
            AND CAST(substr(received_at, -5, 2) AS INTEGER) BETWEEN 0 AND 23
            AND CAST(substr(received_at, -2) AS INTEGER) BETWEEN 0 AND 59
        ))
        AND (
            length(received_at) = 19 + CASE WHEN upper(substr(received_at, -1)) = 'Z' THEN 1 ELSE 6 END
            OR (
                substr(received_at, 20, 1) = '.'
                AND length(received_at) > 20 + CASE WHEN upper(substr(received_at, -1)) = 'Z' THEN 1 ELSE 6 END
                AND substr(received_at, 21, length(received_at) - 20 - CASE WHEN upper(substr(received_at, -1)) = 'Z' THEN 1 ELSE 6 END) NOT GLOB '*[^0-9]*'
            )
        )
    THEN
        unixepoch(substr(received_at, 1, 10) || 'T' || substr(received_at, 12, 6)
            || CASE WHEN substr(received_at, 18, 2) = '60' THEN '59' ELSE substr(received_at, 18, 2) END || 'Z') * 1000
        + CASE WHEN substr(received_at, 18, 2) = '60' THEN 1000 ELSE 0 END
        + CASE WHEN substr(received_at, 20, 1) = '.'
            THEN CAST(substr(substr(received_at, 21, length(received_at) - 20
                - CASE WHEN upper(substr(received_at, -1)) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
            ELSE 0 END
        - CASE WHEN upper(substr(received_at, -1)) = 'Z' THEN 0 ELSE
            (CASE substr(received_at, -6, 1) WHEN '+' THEN 1 ELSE -1 END)
            * (CAST(substr(received_at, -5, 2) AS INTEGER) * 60 + CAST(substr(received_at, -2) AS INTEGER)) * 60000 END
    END
) VIRTUAL NOT NULL;

CREATE INDEX measurements_received_time ON measurements(received_at_ms DESC, id DESC);
CREATE INDEX measurements_display_time ON measurements(coalesce(measured_at_ms, received_at_ms) DESC, id DESC);
CREATE INDEX measurements_metric_time ON measurements(
    astronaut_id, metric, coalesce(measured_at_ms, received_at_ms) DESC, id DESC
);

-- Legacy tasks remain available until the dated HTTP contract is introduced.
CREATE TABLE task_templates (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL
);
INSERT INTO task_templates(id, title) SELECT id, title FROM tasks;

CREATE TABLE task_completions (
    astronaut_id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES task_templates(id),
    mission_date TEXT NOT NULL CHECK (
        mission_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
        AND date(mission_date, '+0 days') IS NOT NULL
        AND date(mission_date, '+0 days') = mission_date
    ),
    completed_at TEXT NOT NULL,
    completed_at_ms INTEGER NOT NULL,
    PRIMARY KEY (astronaut_id, task_id, mission_date)
);
CREATE INDEX task_completions_day ON task_completions(astronaut_id, mission_date, task_id);

-- Preserve an explicit audit record rather than invent a mission date.
CREATE TABLE task_completion_migration_issues (
    task_id TEXT PRIMARY KEY NOT NULL,
    completed_at TEXT NOT NULL,
    reason TEXT NOT NULL
);

CREATE TEMP TABLE legacy_completion_times AS
SELECT id AS task_id, completed_at,
    CASE WHEN
        substr(completed_at, 1, 19) GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9][Tt ][0-9][0-9]:[0-9][0-9]:[0-9][0-9]'
        AND date(substr(completed_at, 1, 10), '+0 days') = substr(completed_at, 1, 10)
        AND CAST(substr(completed_at, 12, 2) AS INTEGER) BETWEEN 0 AND 23
        AND CAST(substr(completed_at, 15, 2) AS INTEGER) BETWEEN 0 AND 59
        AND CAST(substr(completed_at, 18, 2) AS INTEGER) BETWEEN 0 AND 60
        AND (upper(substr(completed_at, -1)) = 'Z' OR (
            substr(completed_at, -6) GLOB '[+−-][0-9][0-9]:[0-9][0-9]'
            AND CAST(substr(completed_at, -5, 2) AS INTEGER) BETWEEN 0 AND 23
            AND CAST(substr(completed_at, -2) AS INTEGER) BETWEEN 0 AND 59
        ))
        AND (
            length(completed_at) = 19 + CASE WHEN upper(substr(completed_at, -1)) = 'Z' THEN 1 ELSE 6 END
            OR (
                substr(completed_at, 20, 1) = '.'
                AND length(completed_at) > 20 + CASE WHEN upper(substr(completed_at, -1)) = 'Z' THEN 1 ELSE 6 END
                AND substr(completed_at, 21, length(completed_at) - 20 - CASE WHEN upper(substr(completed_at, -1)) = 'Z' THEN 1 ELSE 6 END) NOT GLOB '*[^0-9]*'
            )
        )
    THEN
        unixepoch(substr(completed_at, 1, 10) || 'T' || substr(completed_at, 12, 6)
            || CASE WHEN substr(completed_at, 18, 2) = '60' THEN '59' ELSE substr(completed_at, 18, 2) END || 'Z') * 1000
        + CASE WHEN substr(completed_at, 18, 2) = '60' THEN 1000 ELSE 0 END
        + CASE WHEN substr(completed_at, 20, 1) = '.'
            THEN CAST(substr(substr(completed_at, 21, length(completed_at) - 20
                - CASE WHEN upper(substr(completed_at, -1)) = 'Z' THEN 1 ELSE 6 END) || '000', 1, 3) AS INTEGER)
            ELSE 0 END
        - CASE WHEN upper(substr(completed_at, -1)) = 'Z' THEN 0 ELSE
            (CASE substr(completed_at, -6, 1) WHEN '+' THEN 1 ELSE -1 END)
            * (CAST(substr(completed_at, -5, 2) AS INTEGER) * 60 + CAST(substr(completed_at, -2) AS INTEGER)) * 60000 END
    END AS completed_at_ms
FROM tasks WHERE completed_at IS NOT NULL;

-- Leap seconds retain their civil date as in Chrono (23:59:60 is still that day).
-- Floor division is explicit because SQLite INTEGER division truncates toward zero.
CREATE TEMP TABLE legacy_completion_dates AS
SELECT *, date(
    (civil_ms - ((civil_ms % 1000 + 1000) % 1000)) / 1000, 'unixepoch'
) AS mission_date
FROM (
    SELECT *, completed_at_ms - CASE WHEN substr(completed_at, 18, 2) = '60' THEN 1000 ELSE 0 END AS civil_ms
    FROM legacy_completion_times
);

-- Version 0001 had no astronaut on tasks; the application has one demo astronaut.
INSERT INTO task_completions(astronaut_id, task_id, mission_date, completed_at, completed_at_ms)
SELECT 'demo-astronaut', task_id, mission_date, completed_at, completed_at_ms
FROM legacy_completion_dates
WHERE completed_at_ms IS NOT NULL
    AND mission_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]';

INSERT INTO task_completion_migration_issues(task_id, completed_at, reason)
SELECT task_id, completed_at, 'Cannot recover a UTC mission date from legacy completed_at'
FROM legacy_completion_dates
WHERE completed_at_ms IS NULL OR mission_date IS NULL
    OR mission_date NOT GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]';

DROP TABLE legacy_completion_dates;
DROP TABLE legacy_completion_times;

