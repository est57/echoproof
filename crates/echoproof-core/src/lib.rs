//! Core of Echoproof: the snapshot format, sanitization, semantic diff and reports.

pub mod config;
pub mod diff;
pub mod endpoint;
pub mod glob;
pub mod idmap;
#[cfg(test)]
mod idmap_tests;
pub mod intent;
pub mod noise;
pub mod origin;
pub mod outbound;
pub mod render;
pub mod report;
pub mod sanitize;
pub mod snapshot;
