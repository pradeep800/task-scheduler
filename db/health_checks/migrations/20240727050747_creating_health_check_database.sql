CREATE TABLE health_checks(
    task_id INTEGER NOT NULL PRIMARY KEY,
    last_time_health_check TIMESTAMPTZ NOT NULL,
    task_completed BOOLEAN NOT NULL DEFAULT FALSE
);
