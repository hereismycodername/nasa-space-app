//! AstroCheck interface model: the `Backend` contract, plain data types and a demo backend.

mod content;
mod rules;

pub mod anatomy;
pub mod backend;
pub mod demo;
pub mod types;

pub use backend::Backend;
pub use demo::DemoBackend;
pub use rules::fmt as fmt_num;
pub use types::*;
