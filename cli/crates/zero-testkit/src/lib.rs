//! Test doubles for the ZERO engine.
//!
//! The CLI target never ships with this crate. It powers integration
//! tests, snapshot tests, and the performance harness. The mock
//! engine mirrors the real FastAPI shapes — if a schema drifts in
//! production, tests fail loud.

#![allow(clippy::module_name_repetitions)]

pub mod fixtures;
pub mod mock_engine;
