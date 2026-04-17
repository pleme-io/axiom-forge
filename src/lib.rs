//! axiom-forge — Lisp → Rust axiom generation with invariant proofs.
//!
//! The self-hosting primitive layer of the pleme-io platform. Takes an
//! [`AxiomProposal`] (a typed sexpr-expressible record), runs a suite
//! of structural proofs, and emits Rust source for a new enum variant
//! + its sexpr round-trip machinery + frozen test vectors.
//!
//! # Companion document
//!
//! `iac-forge/docs/METACIRCULAR.md` describes the loop this crate
//! realizes. In one sentence: **Lisp generates Rust generates Lisp.**
//!
//! # Pipeline
//!
//! ```text
//!   AxiomProposal (typed sexpr value)
//!         │
//!         ▼  verify  (structural invariants — see `verify` module)
//!     Certificate  OR  Vec<Violation>
//!         │
//!         ▼  emit    (string-template codegen — see `emit` module)
//!     Rust source text  (deterministic, content-hashed)
//!         │
//!         ▼  compile (rustc — the final gatekeeper)
//!     New axiom variant lives in the workspace
//! ```
//!
//! # Why string templates instead of `quote!`
//!
//! The core emission is intentionally dependency-minimal. We emit
//! Rust source as text via small helper functions. This:
//!
//! - Keeps the crate audit-friendly (no proc-macro / syn toolchain
//!   at the core)
//! - Makes emitted output inspectable as pure text (easier to diff,
//!   easier to content-hash, easier for humans to review)
//! - Keeps the proofs pure: the prover reads the sexpr proposal and
//!   decides yes/no without needing syn's semantic analysis
//!
//! A downstream consumer can add `syn`/`quote` on top if they want
//! full AST-level emission. The core stays minimal.
//!
//! # Safety claim
//!
//! **Every emitted axiom is double-gated.** First: axiom-forge's
//! own invariant prover refuses to emit Rust source for a proposal
//! that violates any of the seven obligations (see
//! [`verify::ProofObligation`]). Second: even if emission succeeds,
//! `rustc` is the final gatekeeper — if the generated code doesn't
//! type-check, the axiom literally does not exist in the workspace.
//! The whole pipeline cannot produce invalid Rust that ships.

pub mod proposal;
pub mod verify;
pub mod emit;
pub mod vector;
pub mod primitive_growth;

pub use proposal::{AxiomKind, AxiomProposal, FieldSpec, FieldTy};
pub use verify::{verify, Certificate, ProofObligation, Violation};
pub use emit::{emit_rust, EmissionError, EmissionOutput};
pub use vector::FrozenVector;
pub use primitive_growth::{
    AxiomArtifact, AxiomForge, AxiomGrowthError, GrowthOutcome, PrimitiveGrowth, StepResult,
};
