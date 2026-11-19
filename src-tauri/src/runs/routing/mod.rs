//! QO-06: model / effort routing for delegated work, in three layers.
//!
//! 1. The agent decides: a new Claude session gets a short routing policy
//!    appended to its system prompt ([`policy`]), asking it to set `model` and
//!    `effort` on every `Agent` call and `session_spawn` by the kind of work.
//! 2. The app's fallbacks (`runs::subagent_env`, `conductor::defaults`) apply
//!    when it sets nothing.
//! 3. The app records every decision and how it ended, read from the stream it
//!    already receives — no model call, no extra process.
//!
//! [`tier`] holds the pure vocabulary all layers share (model families, effort
//! levels, task kinds).

pub mod commands;
pub mod escalate;
pub mod policy;
pub mod record;
pub mod report;
pub mod tier;
pub mod worker;
