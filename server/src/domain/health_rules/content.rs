//! Content and thresholds for be08-demo-1; provenance is documented in be08-rules.md.
use super::{FlightEducation, RuleStatus};

pub(super) const DISCLAIMER: &str = "Demonstration support for self-monitoring, not a diagnosis and not a NASA medical protocol. Terrestrial reference values do not reflect the crew's individual plan; the flight surgeon's instructions take priority. These data cannot establish the cause of a change. The app does not prescribe medication and does not send messages to anyone on its own.";
pub(super) const EMERGENCY_GUIDANCE: &str = "For chest pain, marked shortness of breath, loss of consciousness, sudden weakness or impaired speech or vision, involve the crew immediately and follow the mission's emergency medical protocol. Do not wait for an app update, a repeat measurement or a reply from Earth.";

const NASA_HAZARDS: &str = "https://www.nasa.gov/hrp/hazards/";
const NASA_RISKS: &str = "https://www.nasa.gov/hrp/risks/";
const NASA_BODY: &str = "https://www.nasa.gov/humans-in-space/the-human-body-in-space/";
const NASA_ISOLATION: &str = "https://www.nasa.gov/hrp/hazard-isolation-and-confinement/";
const AHA_PULSE: &str = "https://www.heart.org/en/health-topics/high-blood-pressure/the-facts-about-high-blood-pressure/all-about-heart-rate-pulse";
const AHA_PRESSURE: &str = "https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings";
const AHA_URGENT_PRESSURE: &str = "https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings/when-to-call-911-for-high-blood-pressure";
const AHA_LOW_PRESSURE: &str = "https://www.heart.org/en/health-topics/high-blood-pressure/the-facts-about-high-blood-pressure/low-blood-pressure-when-blood-pressure-is-too-low";
const AHA_MEASURE_PRESSURE: &str = "https://www.heart.org/en/health-topics/high-blood-pressure/understanding-blood-pressure-readings/monitoring-your-blood-pressure-at-home";
const NHS_FEVER: &str = "https://www.nhs.uk/symptoms/fever-in-adults/";
const NHS_HYPOTHERMIA: &str = "https://www.nhs.uk/conditions/hypothermia/";
const CDC_SLEEP: &str = "https://www.cdc.gov/sleep/about/";

const PRESSURE_CONTEXT: &str = "Altered gravity affects circulation. Blood pressure helps to observe changes but does not explain their cause and does not measure the state of all organs.";
const PRESSURE_LIMITATION: &str = "This is one part of blood pressure; it does not establish a diagnosis or an overall blood-pressure category. Both parts and the measurement conditions are needed.";
const PRESSURE_SOURCES: &[&str] = &[
    AHA_PRESSURE,
    AHA_URGENT_PRESSURE,
    AHA_LOW_PRESSURE,
    AHA_MEASURE_PRESSURE,
    NASA_BODY,
];
const PRESSURE_REPEAT: &str = "Recheck both parts of blood pressure following the device instructions after quiet rest. Record the conditions; if the deviation repeats, discuss the result with the flight surgeon. If symptoms are marked, involve the crew immediately.";
const PRESSURE_URGENT: &str = "For chest pain, marked shortness of breath, sudden weakness, impaired speech or vision or loss of consciousness, involve the crew immediately and follow the emergency protocol without waiting for a repeat measurement or a reply from Earth. Inform the person responsible for medical care at once. If there are no alarming symptoms, repeat both parts of blood pressure after at least one minute; if the value stays high, urgent medical consultation is needed.";
const ROUTINE: &str = "Continue observation and the assigned mission plan. Seek help for new symptoms regardless of this result.";

#[derive(Clone, Copy)]
enum RuleKind {
    HeartRate,
    SystolicPressure,
    DiastolicPressure,
    Temperature,
    Sleep,
    Mood,
}

pub(super) struct Definition {
    kind: RuleKind,
    pub title: &'static str,
    pub flight_context: &'static str,
    pub limitation: &'static str,
    pub source_urls: &'static [&'static str],
}

pub(super) struct Outcome {
    pub status: RuleStatus,
    pub criterion: &'static str,
    pub suggested_action: &'static str,
    pub escalation: &'static str,
}

fn outcome(
    status: RuleStatus,
    criterion: &'static str,
    suggested_action: &'static str,
    escalation: &'static str,
) -> Outcome {
    Outcome {
        status,
        criterion,
        suggested_action,
        escalation,
    }
}

impl Definition {
    pub fn classify(&self, value: f64) -> Outcome {
        use RuleStatus::{Attention, Critical, NoAlert};
        match self.kind {
            RuleKind::HeartRate if !(60.0..=100.0).contains(&value) => outcome(
                Attention,
                "The resting heart rate reference triggered: <60 or >100 bpm.",
                "Repeat the measurement at rest and note recent exertion and how you feel. If the deviation repeats, discuss it with the flight surgeon; if symptoms are marked, involve the crew immediately.",
                "ground",
            ),
            RuleKind::HeartRate => outcome(NoAlert, "The <60 or >100 bpm reference did not trigger.", ROUTINE, "none"),
            RuleKind::SystolicPressure if value > 180.0 => outcome(Critical, "Urgent threshold triggered: systolic pressure >180 mmHg.", PRESSURE_URGENT, "urgent"),
            RuleKind::SystolicPressure if value < 90.0 => outcome(Attention, "Low systolic pressure reference triggered: <90 mmHg.", PRESSURE_REPEAT, "ground"),
            RuleKind::SystolicPressure if value >= 130.0 => outcome(Attention, "Systolic pressure >=130 mmHg; the value needs a repeat check and a discussion with a medic.", PRESSURE_REPEAT, "ground"),
            RuleKind::SystolicPressure if value >= 120.0 => outcome(Attention, "Systolic pressure >=120 and <130 mmHg; the attention reference triggered.", "Repeat both parts of blood pressure following the device instructions after quiet rest. Keep to the agreed activity, nutrition and sleep plan; if it repeats, discuss the result with a medic.", "self"),
            RuleKind::SystolicPressure => outcome(NoAlert, "The <90 or >=120 mmHg thresholds did not trigger.", "Measure both parts of blood pressure together per the mission plan. Seek help for symptoms regardless of the result.", "none"),
            RuleKind::DiastolicPressure if value > 120.0 => outcome(Critical, "Urgent threshold triggered: diastolic pressure >120 mmHg.", PRESSURE_URGENT, "urgent"),
            RuleKind::DiastolicPressure if value < 60.0 => outcome(Attention, "Low diastolic pressure reference triggered: <60 mmHg.", PRESSURE_REPEAT, "ground"),
            RuleKind::DiastolicPressure if value >= 80.0 => outcome(Attention, "Diastolic pressure reference >=80 mmHg triggered.", PRESSURE_REPEAT, "ground"),
            RuleKind::DiastolicPressure => outcome(NoAlert, "The <60 or >=80 mmHg thresholds did not trigger.", "Measure both parts of blood pressure together per the mission plan. Seek help for symptoms regardless of the result.", "none"),
            RuleKind::Temperature if value < 35.0 => outcome(Critical, "Urgent temperature threshold <35 °C triggered.", "Involve the crew and medical help immediately and follow the onboard protocol. Check the measurement method, but do not delay help for a repeat result.", "urgent"),
            RuleKind::Temperature if value >= 38.0 => outcome(Attention, "Elevated temperature reference >=38 °C triggered.", "Repeat the measurement following the device instructions and inform the person responsible for medical care if the temperature persists or you feel worse. If symptoms are marked, involve the crew immediately.", "ground"),
            RuleKind::Temperature => outcome(NoAlert, "The <35 and >=38 °C thresholds did not trigger.", ROUTINE, "none"),
            RuleKind::Sleep if value < 7.0 => outcome(Attention, "Sleep duration <7 hours is below the selected adult reference.", "Plan protected sleep time and discuss the workload with the crew. If the difficulties repeat, contact the flight surgeon or a support specialist.", "crew"),
            RuleKind::Sleep => outcome(NoAlert, "The <7 hours sleep duration reference did not trigger.", "Keep a regular schedule and sleep conditions per the mission plan; tell the crew about sleepiness and reduced performance.", "none"),
            RuleKind::Mood if value < 3.0 => outcome(Attention, "Team threshold triggered: mood <3 of 5. This is an invitation to support, not a diagnostic scale.", "Allow yourself rest and a private conversation with a trusted crew member. If the difficulties persist, ask a support specialist or the flight surgeon for help.", "crew"),
            RuleKind::Mood => outcome(NoAlert, "The team threshold <3 of 5 did not trigger; this is not a diagnostic assessment.", "Plan rest and social time; support is available at any self-rating.", "none"),
        }
    }
}

pub(super) fn definition(metric: &str) -> Option<Definition> {
    let result = match metric {
        "heartRate" => Definition {
            kind: RuleKind::HeartRate,
            title: "Heart rate",
            flight_context: "In altered gravity, circulation adapts. Compare heart rate with exertion and the individual plan; it does not identify the cause of a change.",
            limitation: "The reference applies only at rest; exertion and fitness change heart rate. The app does not know whether you are at rest.",
            source_urls: &[AHA_PULSE, NASA_BODY],
        },
        "systolicPressure" => Definition {
            kind: RuleKind::SystolicPressure,
            title: "Systolic pressure",
            flight_context: PRESSURE_CONTEXT,
            limitation: PRESSURE_LIMITATION,
            source_urls: PRESSURE_SOURCES,
        },
        "diastolicPressure" => Definition {
            kind: RuleKind::DiastolicPressure,
            title: "Diastolic pressure",
            flight_context: PRESSURE_CONTEXT,
            limitation: PRESSURE_LIMITATION,
            source_urls: PRESSURE_SOURCES,
        },
        "temperature" => Definition {
            kind: RuleKind::Temperature,
            title: "Temperature",
            flight_context: "In a closed environment, hygiene and control of ambient conditions matter. A single temperature cannot establish an infection or its cause.",
            limitation: "The result depends on the measurement method and site; the cause of a change is unknown from a single value.",
            source_urls: &[NHS_FEVER, NHS_HYPOTHERMIA, NASA_HAZARDS],
        },
        "sleepHours" => Definition {
            kind: RuleKind::Sleep,
            title: "Sleep",
            flight_context: "Isolation and a disrupted daily rhythm can affect sleep and performance. Protected rest time is part of prevention.",
            limitation: "A terrestrial adult reference does not replace the mission plan. Hours of sleep do not show its quality.",
            source_urls: &[CDC_SLEEP, NASA_ISOLATION],
        },
        "mood" => Definition {
            kind: RuleKind::Mood,
            title: "Mood self-rating",
            flight_context: "Isolation and confined space can strain well-being and relations within the crew. Regular support is available without a diagnostic label.",
            limitation: "NASA does not set this numeric threshold; a self-rating does not establish a psychiatric diagnosis.",
            source_urls: &[NASA_ISOLATION],
        },
        _ => return None,
    };
    Some(result)
}

pub(super) fn education() -> Vec<FlightEducation> {
    vec![
        FlightEducation {
            id: "gravity",
            title: "Altered gravity",
            explanation: "Circulation and the load on bones and muscles change. These measurements do not show bone loss and do not replace dedicated examinations.",
            suggested_action: "Follow the assigned exercise and examination plan; agree changes in load with a medic.",
            source_urls: &[NASA_BODY],
        },
        FlightEducation {
            id: "isolation",
            title: "Isolation and confinement",
            explanation: "Stress and disturbed sleep can affect well-being and performance. A mood self-rating is not a diagnosis.",
            suggested_action: "Set aside time for sleep, rest and social contact; ask for support when things are difficult.",
            source_urls: &[NASA_ISOLATION],
        },
        FlightEducation {
            id: "radiation",
            title: "Space radiation",
            explanation: "Radiation risks need separate monitoring. Heart rate, temperature and mood do not measure dose and do not diagnose radiation injury.",
            suggested_action: "Use the mission's dosimetry and radiation protection procedures.",
            source_urls: &[NASA_HAZARDS, NASA_RISKS],
        },
        FlightEducation {
            id: "distance",
            title: "Distance from Earth",
            explanation: "Consultation and delivery of help may be delayed. Waiting for a link must not delay crew action in an urgent condition.",
            suggested_action: "For urgent symptoms, involve the crew and apply the approved onboard procedures; contact the ground medical team when possible.",
            source_urls: &[NASA_HAZARDS],
        },
        FlightEducation {
            id: "environment",
            title: "Closed environment",
            explanation: "Air, noise and microorganisms need monitoring. These metrics do not replace environmental monitoring and do not establish the cause of a temperature change.",
            suggested_action: "Follow the environmental control and hygiene procedures and tell the crew about deviations.",
            source_urls: &[NASA_HAZARDS],
        },
    ]
}
