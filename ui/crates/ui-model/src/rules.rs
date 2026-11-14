//! Deterministic threshold rules used by the demo backend (the real backend owns its own).

use crate::content::{EnvDef, IndDef};
use crate::types::*;

pub fn thresholds(ind: &IndDef, env: &EnvDef) -> Thresholds {
    if ind.id == "dose" {
        Thresholds { ok: (0.0, env.dose * 1.5), warn: (0.0, env.dose * 5.0) }
    } else {
        Thresholds { ok: (ind.ok[0], ind.ok[1]), warn: (ind.warn[0], ind.warn[1]) }
    }
}

pub fn status(ind: &IndDef, v: f64, env: &EnvDef) -> Status {
    let t = thresholds(ind, env);
    if v >= t.ok.0 && v <= t.ok.1 {
        Status::Normal
    } else if v >= t.warn.0 && v <= t.warn.1 {
        Status::Watch
    } else {
        Status::Act
    }
}

fn decimals(step: f64) -> i32 {
    format!("{}", step).split('.').nth(1).map(|d| d.len() as i32).unwrap_or(0)
}

/// Clamp to the indicator range and round to its step.
pub fn snap(ind: &IndDef, v: f64) -> f64 {
    let v = v.clamp(ind.min, ind.max);
    let p = 10f64.powi(decimals(ind.step));
    (((v / ind.step).round() * ind.step) * p).round() / p
}

pub fn fmt(x: f64) -> String {
    let s = format!("{:.2}", x);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn range_text(ind: &IndDef, env: &EnvDef) -> String {
    let t = thresholds(ind, env).ok;
    if (t.0 == 0.0 && ind.kind == Kind::Scale) || ind.id == "dose" {
        format!("\u{2264} {} {}", fmt(t.1), ind.unit)
    } else {
        format!("{}\u{2013}{} {}", fmt(t.0), fmt(t.1), ind.unit)
    }
}

pub fn score(statuses: &[Status]) -> Option<u8> {
    let s: Vec<u32> = statuses
        .iter()
        .filter(|s| **s != Status::NoData)
        .map(|s| match s {
            Status::Normal => 0,
            Status::Watch => 1,
            _ => 2,
        })
        .collect();
    if s.is_empty() {
        return None;
    }
    let sum: u32 = s.iter().sum();
    let v = 100.0 - 150.0 * sum as f64 / (2.0 * s.len() as f64);
    Some(v.round().max(0.0) as u8)
}

pub fn advice(ind: &IndDef, st: Status, env: &EnvDef, blackout: bool) -> Advice {
    let tgt = range_text(ind, env);
    match st {
        Status::Normal | Status::NoData => Advice {
            headline: format!("Normal (target {}). Keep it up.", tgt),
            actions: vec![],
            escalate: None,
        },
        Status::Watch => Advice {
            headline: format!("Watch \u{2014} suggested self-care (target {})", tgt),
            actions: ind.a1.clone(),
            escalate: None,
        },
        Status::Act => Advice {
            headline: format!("Act now (target {})", tgt),
            actions: ind.a2.clone(),
            escalate: Some(if blackout {
                "No link to ground. Follow the onboard medical protocol \u{2014} this entry is queued for downlink.".into()
            } else {
                format!("Contact the flight surgeon. {}", env.esc)
            }),
        },
    }
}
