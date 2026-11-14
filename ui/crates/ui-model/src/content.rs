//! Medical content tables embedded in the binary (thresholds, texts, environments).

use crate::types::Kind;
use serde::Deserialize;
use std::collections::HashMap;

const RAW: &str = include_str!("../../../data/content.json");
const SYSTEM_ORDER: [&str; 7] = ["brain", "eyes", "heart", "lungs", "gut", "msk", "immune"];
const ENV_ORDER: [&str; 5] = ["iss", "orion", "moon", "mars", "transit"];

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "HAZ")]
    haz: HashMap<String, String>,
    #[serde(rename = "SYSTEMS")]
    systems: HashMap<String, RawSys>,
    #[serde(rename = "ENVS")]
    envs: HashMap<String, RawEnv>,
    #[serde(rename = "INDS")]
    inds: Vec<RawInd>,
}
#[derive(Deserialize)]
struct RawSys {
    name: String,
    #[serde(rename = "fn")]
    func: String,
    haz: Vec<String>,
    effect: String,
    why: String,
}
#[derive(Deserialize)]
struct RawEnv {
    name: String,
    g: String,
    comms: String,
    rtt: f64,
    dose: f64,
    esc: String,
    notes: HashMap<String, String>,
}
#[derive(Deserialize)]
struct RawInd {
    id: String,
    sys: String,
    name: String,
    unit: String,
    #[serde(rename = "type")]
    kind: String,
    min: f64,
    max: f64,
    step: f64,
    freq: u32,
    ok: [f64; 2],
    warn: [f64; 2],
    seed: [f64; 3],
    why: String,
    how: Vec<String>,
    a1: Vec<String>,
    a2: Vec<String>,
}

pub struct SystemDef {
    pub id: String,
    pub name: String,
    pub function: String,
    pub haz: Vec<String>,
    pub effect: String,
    pub why: String,
}
pub struct EnvDef {
    pub id: String,
    pub name: String,
    pub gravity: String,
    pub comms: String,
    pub rtt: f64,
    pub dose: f64,
    pub esc: String,
    pub notes: HashMap<String, String>,
}
pub struct IndDef {
    pub id: String,
    pub sys: String,
    pub name: String,
    pub unit: String,
    pub kind: Kind,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub freq: u32,
    pub ok: [f64; 2],
    pub warn: [f64; 2],
    pub seed: [f64; 3],
    pub why: String,
    pub how: Vec<String>,
    pub a1: Vec<String>,
    pub a2: Vec<String>,
}
pub struct Content {
    pub hazards: HashMap<String, String>,
    pub systems: Vec<SystemDef>,
    pub envs: Vec<EnvDef>,
    pub inds: Vec<IndDef>,
}

/// Strip emoji / pictographs: they render as boxes on minimal fonts and some terminals.
pub fn plain(s: &str) -> String {
    let t: String = s
        .chars()
        .filter(|&c| {
            let u = c as u32;
            !(u >= 0x1F000 || (0x2600..=0x27BF).contains(&u) || u == 0xFE0F || u == 0x200D)
        })
        .collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn pv(v: Vec<String>) -> Vec<String> {
    v.iter().map(|s| plain(s)).collect()
}

impl Content {
    pub fn load() -> Content {
        let mut raw: Raw = serde_json::from_str(RAW).expect("embedded content.json is valid");
        let systems = SYSTEM_ORDER
            .iter()
            .map(|id| {
                let s = raw.systems.remove(*id).expect("system present");
                SystemDef {
                    id: id.to_string(),
                    name: plain(&s.name),
                    function: plain(&s.func),
                    haz: s.haz,
                    effect: plain(&s.effect),
                    why: plain(&s.why),
                }
            })
            .collect();
        let envs = ENV_ORDER
            .iter()
            .map(|id| {
                let e = raw.envs.remove(*id).expect("environment present");
                EnvDef {
                    id: id.to_string(),
                    name: plain(&e.name),
                    gravity: plain(&e.g),
                    comms: plain(&e.comms),
                    rtt: e.rtt,
                    dose: e.dose,
                    esc: plain(&e.esc),
                    notes: e.notes.into_iter().map(|(k, v)| (k, plain(&v))).collect(),
                }
            })
            .collect();
        let inds = raw
            .inds
            .into_iter()
            .map(|i| IndDef {
                id: i.id,
                sys: i.sys,
                name: plain(&i.name),
                unit: plain(&i.unit),
                kind: if i.kind == "scale" { Kind::Scale } else { Kind::Num },
                min: i.min,
                max: i.max,
                step: i.step,
                freq: i.freq,
                ok: i.ok,
                warn: i.warn,
                seed: i.seed,
                why: plain(&i.why),
                how: pv(i.how),
                a1: pv(i.a1),
                a2: pv(i.a2),
            })
            .collect();
        Content {
            hazards: raw.haz.into_iter().map(|(k, v)| (k, plain(&v))).collect(),
            systems,
            envs,
            inds,
        }
    }
}
