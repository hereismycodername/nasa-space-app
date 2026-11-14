//! Content and thresholds for be08-demo-1; provenance is documented in be08-rules.md.
use super::{FlightEducation, RuleStatus};

pub(super) const DISCLAIMER: &str = "Демонстрационная поддержка самонаблюдения, не диагноз и не медицинский протокол NASA. Земные ориентиры не учитывают индивидуальный план экипажа; указания полётного врача имеют приоритет. Причину изменения по этим данным установить нельзя. Приложение не назначает лекарства и само никому не отправляет сообщения.";
pub(super) const EMERGENCY_GUIDANCE: &str = "При боли в груди, выраженной одышке, потере сознания, внезапной слабости или нарушении речи/зрения немедленно привлеките экипаж и действуйте по аварийному медицинскому протоколу миссии. Не ждите обновления приложения, повторного измерения или ответа с Земли.";

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

const PRESSURE_CONTEXT: &str = "Изменённая гравитация влияет на кровообращение. Давление помогает наблюдать изменения, но не объясняет их причину и не измеряет состояние всех органов.";
const PRESSURE_LIMITATION: &str = "Это отдельная часть давления; диагноз и общая категория давления по ней не устанавливаются. Нужны обе части и условия измерения.";
const PRESSURE_SOURCES: &[&str] = &[
    AHA_PRESSURE,
    AHA_URGENT_PRESSURE,
    AHA_LOW_PRESSURE,
    AHA_MEASURE_PRESSURE,
    NASA_BODY,
];
const PRESSURE_REPEAT: &str = "Перепроверьте обе части давления по инструкции прибора после спокойного отдыха. Запишите условия; при повторном отклонении обсудите результат с полётным врачом. При выраженных симптомах немедленно привлеките экипаж.";
const PRESSURE_URGENT: &str = "При боли в груди, выраженной одышке, внезапной слабости, нарушении речи/зрения или потере сознания немедленно привлеките экипаж и действуйте по аварийному протоколу, не ожидая повторного измерения или ответа с Земли. Сразу сообщите ответственному за медпомощь. Если тревожных симптомов нет, повторите обе части давления не раньше чем через минуту; при сохранении высокого значения нужна срочная медицинская консультация.";
const ROUTINE: &str = "Продолжайте наблюдение и назначенный план миссии. При новых симптомах обращайтесь за помощью независимо от этого результата.";

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
                "Сработал ориентир для пульса в покое: <60 или >100 bpm.",
                "Повторите измерение в спокойном состоянии, отметьте недавнюю нагрузку и самочувствие. При повторении отклонения обсудите его с полётным врачом; при выраженных симптомах немедленно привлеките экипаж.",
                "ground",
            ),
            RuleKind::HeartRate => outcome(NoAlert, "Ориентир <60 или >100 bpm не сработал.", ROUTINE, "none"),
            RuleKind::SystolicPressure if value > 180.0 => outcome(Critical, "Сработал срочный порог: систолическое давление >180 mmHg.", PRESSURE_URGENT, "urgent"),
            RuleKind::SystolicPressure if value < 90.0 => outcome(Attention, "Сработал ориентир низкого систолического давления: <90 mmHg.", PRESSURE_REPEAT, "ground"),
            RuleKind::SystolicPressure if value >= 130.0 => outcome(Attention, "Систолическое давление >=130 mmHg; значение требует повторной проверки и обсуждения с медиком.", PRESSURE_REPEAT, "ground"),
            RuleKind::SystolicPressure if value >= 120.0 => outcome(Attention, "Систолическое давление >=120 и <130 mmHg; сработал ориентир для внимания.", "Повторите обе части давления по инструкции прибора после спокойного отдыха. Соблюдайте согласованный план активности, питания и сна; при повторении обсудите результат с медиком.", "self"),
            RuleKind::SystolicPressure => outcome(NoAlert, "Пороги <90 или >=120 mmHg не сработали.", "Измеряйте обе части давления вместе по плану миссии. При симптомах обращайтесь за помощью независимо от результата.", "none"),
            RuleKind::DiastolicPressure if value > 120.0 => outcome(Critical, "Сработал срочный порог: диастолическое давление >120 mmHg.", PRESSURE_URGENT, "urgent"),
            RuleKind::DiastolicPressure if value < 60.0 => outcome(Attention, "Сработал ориентир низкого диастолического давления: <60 mmHg.", PRESSURE_REPEAT, "ground"),
            RuleKind::DiastolicPressure if value >= 80.0 => outcome(Attention, "Сработал ориентир диастолического давления >=80 mmHg.", PRESSURE_REPEAT, "ground"),
            RuleKind::DiastolicPressure => outcome(NoAlert, "Пороги <60 или >=80 mmHg не сработали.", "Измеряйте обе части давления вместе по плану миссии. При симптомах обращайтесь за помощью независимо от результата.", "none"),
            RuleKind::Temperature if value < 35.0 => outcome(Critical, "Сработал срочный порог температуры <35 °C.", "Немедленно привлеките экипаж и медицинскую помощь, следуйте бортовому протоколу. Проверьте способ измерения, но не откладывайте помощь ради повторного результата.", "urgent"),
            RuleKind::Temperature if value >= 38.0 => outcome(Attention, "Сработал ориентир повышенной температуры >=38 °C.", "Повторите измерение по инструкции прибора и сообщите ответственному за медпомощь при сохранении температуры или ухудшении самочувствия. При выраженных симптомах немедленно привлеките экипаж.", "ground"),
            RuleKind::Temperature => outcome(NoAlert, "Пороги <35 и >=38 °C не сработали.", ROUTINE, "none"),
            RuleKind::Sleep if value < 7.0 => outcome(Attention, "Продолжительность сна <7 часов — ниже выбранного ориентира для взрослых.", "Запланируйте защищённое время для сна и обсудите нагрузку с экипажем. При повторении трудностей обратитесь к полётному врачу или специалисту поддержки.", "crew"),
            RuleKind::Sleep => outcome(NoAlert, "Ориентир продолжительности сна <7 часов не сработал.", "Сохраняйте регулярный график и условия для сна по плану миссии; сообщайте экипажу о сонливости и снижении работоспособности.", "none"),
            RuleKind::Mood if value < 3.0 => outcome(Attention, "Сработал условный порог команды: настроение <3 из 5. Это приглашение к поддержке, не диагностическая шкала.", "Предложите себе отдых и приватный разговор с доверенным членом экипажа. Если трудности сохраняются, запросите помощь специалиста поддержки или полётного врача.", "crew"),
            RuleKind::Mood => outcome(NoAlert, "Условный порог команды <3 из 5 не сработал; это не диагностическая оценка.", "Планируйте отдых и общение; за поддержкой можно обратиться при любом значении самооценки.", "none"),
        }
    }
}

pub(super) fn definition(metric: &str) -> Option<Definition> {
    let result = match metric {
        "heartRate" => Definition {
            kind: RuleKind::HeartRate,
            title: "Пульс",
            flight_context: "В изменённой гравитации кровообращение адаптируется. Сопоставляйте пульс с нагрузкой и индивидуальным планом; он не определяет причину изменения.",
            limitation: "Ориентир применим только в покое; нагрузка и тренированность меняют пульс. Контекст покоя приложению неизвестен.",
            source_urls: &[AHA_PULSE, NASA_BODY],
        },
        "systolicPressure" => Definition {
            kind: RuleKind::SystolicPressure,
            title: "Систолическое давление",
            flight_context: PRESSURE_CONTEXT,
            limitation: PRESSURE_LIMITATION,
            source_urls: PRESSURE_SOURCES,
        },
        "diastolicPressure" => Definition {
            kind: RuleKind::DiastolicPressure,
            title: "Диастолическое давление",
            flight_context: PRESSURE_CONTEXT,
            limitation: PRESSURE_LIMITATION,
            source_urls: PRESSURE_SOURCES,
        },
        "temperature" => Definition {
            kind: RuleKind::Temperature,
            title: "Температура",
            flight_context: "В замкнутой среде важны гигиена и контроль окружающих условий. По одной температуре нельзя установить инфекцию или её причину.",
            limitation: "Результат зависит от способа и места измерения; причина изменения по одному значению неизвестна.",
            source_urls: &[NHS_FEVER, NHS_HYPOTHERMIA, NASA_HAZARDS],
        },
        "sleepHours" => Definition {
            kind: RuleKind::Sleep,
            title: "Сон",
            flight_context: "Изоляция и нарушение привычного ритма могут влиять на сон и работоспособность. Защищённое время отдыха — часть профилактики.",
            limitation: "Земной ориентир для взрослых не заменяет план миссии. Часы сна не показывают его качество.",
            source_urls: &[CDC_SLEEP, NASA_ISOLATION],
        },
        "mood" => Definition {
            kind: RuleKind::Mood,
            title: "Самооценка настроения",
            flight_context: "Изоляция и ограниченное пространство могут осложнять самочувствие и отношения в экипаже. Регулярная поддержка доступна без диагностического ярлыка.",
            limitation: "NASA не задаёт этот числовой порог; самооценка не устанавливает психиатрический диагноз.",
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
            title: "Изменённая гравитация",
            explanation: "Меняются кровообращение и нагрузка на кости и мышцы. Эти измерения не показывают потерю костной ткани и не заменяют профильные исследования.",
            suggested_action: "Следуйте назначенному плану упражнений и обследований; согласовывайте изменения нагрузки с медиком.",
            source_urls: &[NASA_BODY],
        },
        FlightEducation {
            id: "isolation",
            title: "Изоляция и ограниченное пространство",
            explanation: "Стресс и нарушения сна могут влиять на самочувствие и работоспособность. Самооценка настроения не является диагнозом.",
            suggested_action: "Выделяйте время для сна, отдыха и общения; при трудностях запрашивайте поддержку.",
            source_urls: &[NASA_ISOLATION],
        },
        FlightEducation {
            id: "radiation",
            title: "Космическая радиация",
            explanation: "Радиационные риски требуют отдельного контроля. Пульс, температура и настроение не измеряют дозу и не диагностируют лучевое поражение.",
            suggested_action: "Используйте дозиметрию и процедуры радиационной защиты миссии.",
            source_urls: &[NASA_HAZARDS, NASA_RISKS],
        },
        FlightEducation {
            id: "distance",
            title: "Удалённость от Земли",
            explanation: "Консультация и доставка помощи могут задерживаться. Ожидание связи не должно откладывать действия экипажа при срочном состоянии.",
            suggested_action: "При срочных симптомах привлеките экипаж и примените утверждённые бортовые процедуры; свяжитесь с наземной медицинской командой, когда это возможно.",
            source_urls: &[NASA_HAZARDS],
        },
        FlightEducation {
            id: "environment",
            title: "Замкнутая среда",
            explanation: "Воздух, шум и микроорганизмы требуют контроля. Эти показатели не заменяют мониторинг среды и не устанавливают причину изменения температуры.",
            suggested_action: "Соблюдайте процедуры контроля среды и гигиены, сообщайте экипажу об отклонениях.",
            source_urls: &[NASA_HAZARDS],
        },
    ]
}
