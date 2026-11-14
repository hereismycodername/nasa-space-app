-- Metric-specific effective-time queries use measurements_metric_time from 0002.
-- Keep each read scoped to one astronaut, including the all-metric history page.
CREATE INDEX measurements_astronaut_time ON measurements(
    astronaut_id, COALESCE(measured_at_ms, received_at_ms) DESC, id DESC
);
CREATE INDEX measurements_known_metric_time ON measurements(
    astronaut_id, metric, measured_at_ms DESC, id DESC
) WHERE measured_at IS NOT NULL;
CREATE INDEX measurements_metric_received_time ON measurements(
    astronaut_id, metric, received_at_ms DESC, id DESC
);
CREATE INDEX measurements_astronaut_received_time ON measurements(
    astronaut_id, received_at_ms DESC, id DESC
);
