use ui_model::{IndicatorInfo, Kind, SystemInfo};

#[derive(Clone, Copy)]
pub(crate) struct Metric {
    pub id: &'static str,
    pub api: &'static str,
    pub system: &'static str,
    pub name: &'static str,
    pub unit: &'static str,
    pub minimum: f64,
    pub maximum: f64,
}

pub(crate) const METRICS: [Metric; 6] = [
    Metric {
        id: "hr",
        api: "heartRate",
        system: "heart",
        name: "Heart rate",
        unit: "bpm",
        minimum: 0.0,
        maximum: f64::MAX,
    },
    Metric {
        id: "sbp",
        api: "systolicPressure",
        system: "heart",
        name: "Systolic pressure",
        unit: "mmHg",
        minimum: 0.0,
        maximum: f64::MAX,
    },
    Metric {
        id: "dbp",
        api: "diastolicPressure",
        system: "heart",
        name: "Diastolic pressure",
        unit: "mmHg",
        minimum: 0.0,
        maximum: f64::MAX,
    },
    Metric {
        id: "temp",
        api: "temperature",
        system: "immune",
        name: "Body temperature",
        unit: "°C",
        minimum: 0.0,
        maximum: f64::MAX,
    },
    Metric {
        id: "sleep",
        api: "sleepHours",
        system: "brain",
        name: "Sleep duration",
        unit: "h",
        minimum: 0.0,
        maximum: 24.0,
    },
    Metric {
        id: "mood",
        api: "mood",
        system: "brain",
        name: "Mood",
        unit: "score",
        minimum: 1.0,
        maximum: 5.0,
    },
];

pub(crate) fn metric(id: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|item| item.id == id)
}

impl Metric {
    pub fn validate(self, value: f64) -> Result<(), String> {
        let valid = value.is_finite()
            && value <= self.maximum
            && if matches!(self.id, "sleep" | "mood") {
                value >= self.minimum
            } else {
                value > 0.0
            };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "{}: value outside API input bounds ({})",
                self.name,
                if self.id == "sleep" {
                    "0–24 h"
                } else if self.id == "mood" {
                    "1–5 score"
                } else {
                    "finite and greater than zero"
                }
            ))
        }
    }

    pub fn info(self) -> IndicatorInfo {
        IndicatorInfo {
            id: self.id.into(), system: self.system.into(), name: self.name.into(), unit: self.unit.into(),
            kind: Kind::Num, min: self.minimum, max: self.maximum, step: 0.1, freq_days: 0,
            why: "Recorded by the shared server; clinical assessment is not connected.".into(),
            how: vec![format!("Enter the measured value in {}. The input timestamp is the time of submission.", self.unit),
                "Wait for the server confirmation before closing the application.".into()],
        }
    }
}

pub(crate) fn systems() -> Vec<SystemInfo> {
    [
        ("heart", "Heart & circulation"),
        ("immune", "Whole body / temperature"),
        ("brain", "Sleep & mood"),
    ]
    .into_iter()
    .map(|(id, name)| SystemInfo {
        id: id.into(),
        name: name.into(),
        function: "Shared measurements".into(),
        hazards: vec![],
        effect: String::new(),
        why: "Data availability does not indicate health status.".into(),
    })
    .collect()
}
