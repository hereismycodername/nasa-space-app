//! The team backend's domain rules and SQLite storage (branch codex/team-integration), embedded without the HTTP layer.
//! Parts the interface does not use yet (assessments, tasks) are kept as they are for the next steps.
#![allow(dead_code)]

pub(crate) mod domain;
pub(crate) mod storage;
