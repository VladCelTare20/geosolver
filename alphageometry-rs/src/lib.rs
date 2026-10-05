//! # alphageometry-rs
//!
//! A high-performance Rust reimplementation of the DDAR symbolic reasoning core
//! of AlphaGeometry2.
//!
//! DDAR ("Deductive Database + Algebraic Reasoning") proves olympiad geometry
//! facts by (a) using floating-point coordinates as an oracle for which facts
//! hold, and (b) discharging them exactly with Gaussian elimination over three
//! algebraic systems: directed angles (mod a half-turn), multiplicative
//! distances (in log space), and additive segment lengths.
//!
//! The crate is organized bottom-up:
//! * [`rational`] — exact rationals with a machine-word fast path.
//! * [`numerics`] — floating-point Euclidean geometry (the oracle).
//! * [`lincomb`] / [`elim_core`] — sparse linear algebra and Gaussian elimination.
//! * [`elimination`] — the three geometric algebraic systems.
//! * [`predicate`] — the AlphaGeometry predicate/problem language and parser.
//! * [`engine`] — the DDAR deductive-closure loop.

pub(crate) mod aux_rollout;
pub(crate) mod aux_score;
pub mod aux_search;
pub(crate) mod aux_virtual;
pub mod bench;
pub(crate) mod certify;
pub mod corpus;
pub mod elim_core;
pub mod elimination;
pub mod engine;
pub mod fuzz;
pub(crate) mod fingerprint;
pub mod geo;
pub mod human;
pub mod lincomb;
pub mod metric;
pub mod numerics;
pub mod predicate;
pub mod proof;
pub mod quiet_panic;
pub mod ratio;
pub mod rational;
pub mod runner;
pub mod svg;
pub mod synthetic;

pub use engine::Ddar;
pub use predicate::{Predicate, Problem};
