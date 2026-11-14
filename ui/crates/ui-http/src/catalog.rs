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
    pub minimum_inclusive: bool,
    pub required: bool,
}

pub(crate) const METRICS: [Metric; 12] = [
    Metric {
        id: "hr",
        api: "heartRate",
        system: "heart",
        name: "Heart rate",
        unit: "bpm",
        minimum: 0.0,
        maximum: f64::MAX,
        minimum_inclusive: false,
        required: true,
    },
    Metric {
        id: "sbp",
        api: "systolicPressure",
        system: "heart",
        name: "Systolic pressure",
        unit: "mmHg",
        minimum: 0.0,
        maximum: f64::MAX,
        minimum_inclusive: false,
        required: true,
    },
    Metric {
        id: "dbp",
        api: "diastolicPressure",
        system: "heart",
        name: "Diastolic pressure",
        unit: "mmHg",
        minimum: 0.0,
        maximum: f64::MAX,
        minimum_inclusive: false,
        required: true,
    },
    Metric {
        id: "temp",
        api: "temperature",
        system: "immune",
        name: "Body temperature",
        unit: "°C",
        minimum: 0.0,
        maximum: f64::MAX,
        minimum_inclusive: false,
        required: true,
    },
    Metric {
        id: "sleep",
        api: "sleepHours",
        system: "brain",
        name: "Sleep duration",
        unit: "h",
        minimum: 0.0,
        maximum: 24.0,
        minimum_inclusive: true,
        required: true,
    },
    Metric {
        id: "mood",
        api: "mood",
        system: "brain",
        name: "Mood",
        unit: "score",
        minimum: 1.0,
        maximum: 5.0,
        minimum_inclusive: true,
        required: true,
    },
    Metric {
        id: "spo2",
        api: "spo2",
        system: "lungs",
        name: "Oxygen saturation",
        unit: "%",
        minimum: 0.0,
        maximum: 100.0,
        minimum_inclusive: true,
        required: false,
    },
    Metric {
        id: "glucose",
        api: "glucose",
        system: "gut",
        name: "Blood glucose",
        unit: "mg/dL",
        minimum: 0.0,
        maximum: f64::MAX,
        minimum_inclusive: false,
        required: false,
    },
    Metric {
        id: "visionBlur",
        api: "visionBlur",
        system: "eyes",
        name: "Blurred vision",
        unit: "score",
        minimum: 0.0,
        maximum: 10.0,
        minimum_inclusive: true,
        required: false,
    },
    Metric {
        id: "exerciseMinutes",
        api: "exerciseMinutes",
        system: "msk",
        name: "Exercise in the last 24 hours",
        unit: "min",
        minimum: 0.0,
        maximum: 1440.0,
        minimum_inclusive: true,
        required: false,
    },
    Metric {
        id: "headache",
        api: "headache",
        system: "brain",
        name: "Headache",
        unit: "score",
        minimum: 0.0,
        maximum: 10.0,
        minimum_inclusive: true,
        required: false,
    },
    Metric {
        id: "backPain",
        api: "backPain",
        system: "msk",
        name: "Back pain",
        unit: "score",
        minimum: 0.0,
        maximum: 10.0,
        minimum_inclusive: true,
        required: false,
    },
];

pub(crate) fn metric(id: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|item| item.id == id)
}

impl Metric {
    pub fn validate(self, value: f64) -> Result<(), String> {
        let valid = value.is_finite()
            && value <= self.maximum
            && if self.minimum_inclusive {
                value >= self.minimum
            } else {
                value > self.minimum
            };
        if valid {
            Ok(())
        } else {
            Err(format!(
                "{}: value outside API input bounds ({})",
                self.name,
                if self.maximum.is_finite() && self.maximum != f64::MAX {
                    format!("{}–{} {}", self.minimum, self.maximum, self.unit)
                } else {
                    "finite and greater than zero".into()
                }
            ))
        }
    }

    pub fn info(self) -> IndicatorInfo {
        IndicatorInfo {
            id: self.id.into(), system: self.system.into(), name: self.name.into(), unit: self.unit.into(),
            kind: Kind::Num, min: self.minimum, max: self.maximum, step: 0.1, freq_days: 0,
            why: "Recorded by the shared server; health guidance is evaluated separately from data freshness.".into(),
            how: vec![format!("Enter the measured value in {}. The input timestamp is the time of submission.", self.unit),
                "Wait for the server confirmation before closing the application.".into()],
        }
    }
}

pub(crate) fn systems() -> Vec<SystemInfo> {
    [
        ("heart", "Heart & circulation"),
        ("immune", "Whole body / temperature"),
        ("brain", "Sleep, mood & headache"),
        ("lungs", "Breathing / oxygen"),
        ("gut", "Nutrition / glucose"),
        ("eyes", "Vision"),
        ("msk", "Muscles & bones"),
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

/// Optional metrics are enabled only after the server advertises them in latest.
pub(crate) fn available(latest: Option<&crate::dto::Latest>) -> Vec<&'static Metric> {
    METRICS
        .iter()
        .filter(|metric| {
            latest.map_or(metric.required, |latest| {
                latest
                    .metrics
                    .iter()
                    .any(|entry| entry.metric == metric.api)
            })
        })
        .collect()
}
