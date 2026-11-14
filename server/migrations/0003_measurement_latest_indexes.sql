CREATE INDEX measurements_metric_known_time ON measurements(
    astronaut_id, metric, measured_at_ms DESC, id DESC
) WHERE measured_at IS NOT NULL;

CREATE INDEX measurements_metric_received_time ON measurements(
    astronaut_id, metric, received_at_ms DESC, id DESC
);

CREATE INDEX measurements_astronaut_display_time ON measurements(
    astronaut_id, coalesce(measured_at_ms, received_at_ms) DESC, id DESC
);
