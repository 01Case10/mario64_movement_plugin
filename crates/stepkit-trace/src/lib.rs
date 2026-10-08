//! stepkit-trace: trace and scenario schemas, loaders, comparison engine.
//!
//! Canonical traces are CSV with a JSON sidecar (git-friendly). The
//! comparison engine supports free-running and teacher-forced modes and
//! writes Markdown + JSON divergence reports.

pub mod compare;
pub mod pack;
pub mod runner;
pub mod scenario;
pub mod summarize;
pub mod trace;
