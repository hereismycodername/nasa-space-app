//! Mapping between the interface's indicator ids and the metric names of the backend storage.
//!
//! The first four rows reuse metrics that the team backend already defines (heartRate, systolicPressure,
//! temperature, sleepHours); the others are the additional metrics of the interface. Units and bounds are
//! those of `data/content.json`, so a value that the UI can enter is always accepted by the storage.

/// Input bounds of one additional metric (inclusive).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub unit: &'static str,
    pub lo: f64,
    pub hi: f64,
}

/// (indicator id, metric name)
pub const MAP: &[(&str, &str)] = &[
    ("hr", "heartRate"),
    ("sbp", "systolicPressure"),
    ("temp", "temperature"),
    ("sleep", "sleepHours"),
    ("spo2", "bloodOxygen"),
    ("rr", "respiratoryRate"),
    ("glucose", "glucose"),
    ("dose", "radiationDose"),
    ("pvt", "reactionTime"),
    ("headache", "headache"),
    ("stress", "stress"),
    ("vision", "vision"),
    ("urine", "urineColour"),
    ("mass", "massChange"),
    ("exercise", "exercise"),
    ("back", "backPain"),
];

pub fn metric_of(indicator: &str) -> Option<&'static str> {
    MAP.iter().find(|(i, _)| *i == indicator).map(|(_, m)| *m)
}

pub fn indicator_of(metric: &str) -> Option<&'static str> {
    MAP.iter().find(|(_, m)| *m == metric).map(|(i, _)| *i)
}

/// Unit string of any metric the UI uses.
pub fn unit(metric: &str) -> Option<&'static str> {
    match metric {
        "heartRate" => Some("bpm"),
        "systolicPressure" => Some("mmHg"),
        "temperature" => Some("\u{b0}C"),
        "sleepHours" => Some("h"),
        other => spec(other).map(|s| s.unit),
    }
}

/// Bounds for the additional metrics; the team's own metrics keep their original rules.
pub fn spec(metric: &str) -> Option<Spec> {
    let s = |unit, lo, hi| Some(Spec { unit, lo, hi });
    match metric {
        "bloodOxygen" => s("%", 80.0, 100.0),
        "respiratoryRate" => s("/min", 6.0, 40.0),
        "glucose" => s("mg/dL", 40.0, 300.0),
        "radiationDose" => s("mSv/day", 0.0, 12.0),
        "reactionTime" => s("ms", 150.0, 700.0),
        "headache" | "stress" | "vision" | "backPain" => s("/10", 0.0, 10.0),
        "urineColour" => s("/8", 1.0, 8.0),
        "massChange" => s("%", -15.0, 10.0),
        "exercise" => s("min", 0.0, 240.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_indicator_maps_both_ways_and_is_known_to_the_storage() {
        let own = ["heartRate", "systolicPressure", "temperature", "sleepHours"];
        for (ind, metric) in MAP {
            assert_eq!(indicator_of(metric), Some(*ind));
            assert!(own.contains(metric) || spec(metric).is_some(), "{metric}");
        }
        assert_eq!(MAP.len(), 16);
    }
}
