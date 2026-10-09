//! The only ClearSpace component permitted to delete, move or modify user files.
//!
//! Milestone (a) contains no production deletion code yet: only the Recycle Bin spike in
//! `examples/recycle_spike.rs`, which operates exclusively on sandbox files it creates
//! itself. The real cleaner (milestone e) will accept nothing but a `DeletionPlan` built from
//! the user's final confirmation; see `SAFETY.md`.
//!
//! This crate is the single exemption from the workspace-wide `disallowed_methods` deny
//! (see `clippy.toml`).

#![allow(clippy::disallowed_methods)]
