//! `PrimitiveGrowth` — axiom-forge as an instance of the shared pattern.
//!
//! Third-domain validation after mathscape (Term substrate) and
//! ml-forge (Graph substrate). axiom-forge implements the pattern at
//! the *primitive-minting* level itself: proposals are `AxiomProposal`,
//! certificates are axiom-forge's own `Certificate`, artifacts are
//! the emitted Rust source + FrozenVector pair.
//!
//! The trait below is a local mirror of the canonical trait (which
//! currently lives bundled inside the mathscape workspace's
//! `primitive-forge` crate). Once primitive-forge has its own git
//! repo, this module will switch to `impl primitive_forge::PrimitiveGrowth`
//! without behavioral change.
//!
//! # Why axiom-forge implements the pattern
//!
//! The symmetry is clean: axiom-forge is already the machinery that
//! takes a typed proposal, runs structural proofs, and emits a
//! content-addressable artifact. Wrapping that as a PrimitiveGrowth
//! instance means any upstream generator (mathscape, ml-forge,
//! hand-typed sexpr, future learning systems) can drive axiom-forge
//! through the same four-role interface it already implements
//! informally.

use crate::emit::{emit_rust, EmissionError, EmissionOutput};
use crate::proposal::AxiomProposal;
use crate::vector::FrozenVector;
use crate::verify::{verify, Certificate, VerifyConfig, Violation};

/// Local mirror of `primitive_forge::PrimitiveGrowth`.
pub trait PrimitiveGrowth {
    type Proposal;
    type Certificate;
    type Violations;
    type Artifact;
    type CommittedId;

    fn propose(&mut self) -> Vec<Self::Proposal>;
    fn prove(
        &self,
        proposal: &Self::Proposal,
    ) -> GrowthOutcome<Self::Certificate, Self::Violations>;
    fn emit(
        &self,
        proposal: &Self::Proposal,
        certificate: &Self::Certificate,
    ) -> Option<Self::Artifact>;
    fn register(&mut self, artifact: Self::Artifact) -> Self::CommittedId;

    fn step(&mut self) -> Vec<StepResult<Self::CommittedId, Self::Violations>> {
        let proposals = self.propose();
        let mut results = Vec::with_capacity(proposals.len());
        for proposal in &proposals {
            match self.prove(proposal) {
                GrowthOutcome::Accept(cert) => match self.emit(proposal, &cert) {
                    Some(artifact) => {
                        let id = self.register(artifact);
                        results.push(StepResult::Registered(id));
                    }
                    None => results.push(StepResult::EmittedNone),
                },
                GrowthOutcome::Reject(violations) => {
                    results.push(StepResult::Rejected(violations));
                }
            }
        }
        results
    }
}

pub enum GrowthOutcome<Cert, Viol> {
    Accept(Cert),
    Reject(Viol),
}

pub enum StepResult<Id, Viol> {
    Registered(Id),
    EmittedNone,
    Rejected(Viol),
}

// ────────────────────────────────────────────────────────────────────
// axiom-forge implementation
// ────────────────────────────────────────────────────────────────────

/// A fully-materialized axiom promotion: the original proposal,
/// axiom-forge's Certificate, the emitted Rust source, and the
/// FrozenVector pinning cross-language portability.
#[derive(Debug, Clone)]
pub struct AxiomArtifact {
    pub proposal: AxiomProposal,
    pub certificate: Certificate,
    pub emission: EmissionOutput,
    pub frozen_vector: FrozenVector,
}

/// axiom-forge as a PrimitiveGrowth instance. Holds a queue of
/// pending proposals + a registry of successfully-emitted artifacts.
/// Real callers (mathscape's bridge, future ml-forge UOp proposer)
/// drive this by pushing proposals and calling step().
pub struct AxiomForge {
    pub pending: Vec<AxiomProposal>,
    pub registry: Vec<AxiomArtifact>,
    pub verify_config: VerifyConfig,
}

impl AxiomForge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            registry: Vec::new(),
            verify_config: VerifyConfig::default(),
        }
    }

    pub fn enqueue(&mut self, proposal: AxiomProposal) {
        self.pending.push(proposal);
    }
}

impl Default for AxiomForge {
    fn default() -> Self {
        Self::new()
    }
}

impl PrimitiveGrowth for AxiomForge {
    type Proposal = AxiomProposal;
    type Certificate = Certificate;
    type Violations = Vec<Violation>;
    type Artifact = AxiomArtifact;
    type CommittedId = String; // the axiom's name (PascalCase variant)

    fn propose(&mut self) -> Vec<Self::Proposal> {
        std::mem::take(&mut self.pending)
    }

    fn prove(
        &self,
        proposal: &Self::Proposal,
    ) -> GrowthOutcome<Self::Certificate, Self::Violations> {
        match verify(proposal, &self.verify_config) {
            Ok(cert) => GrowthOutcome::Accept(cert),
            Err(violations) => GrowthOutcome::Reject(violations),
        }
    }

    fn emit(
        &self,
        proposal: &Self::Proposal,
        certificate: &Self::Certificate,
    ) -> Option<Self::Artifact> {
        let emission: EmissionOutput = match emit_rust(proposal, certificate) {
            Ok(e) => e,
            Err(_e) => return None,
        };
        let frozen_vector = FrozenVector::from_emission(proposal, certificate, &emission);
        Some(AxiomArtifact {
            proposal: proposal.clone(),
            certificate: certificate.clone(),
            emission,
            frozen_vector,
        })
    }

    fn register(&mut self, artifact: Self::Artifact) -> Self::CommittedId {
        let id = artifact.proposal.name.clone();
        self.registry.push(artifact);
        id
    }
}

/// Error type for bubbling EmissionError out via the PrimitiveGrowth
/// surface. Currently unused (emit returns Option<Artifact> per the
/// trait) but exposed for callers that want explicit failure info.
#[derive(Debug)]
pub enum AxiomGrowthError {
    Emission(EmissionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::{AxiomKind, AxiomProposal};

    fn good_proposal() -> AxiomProposal {
        AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "TouchTimestamp",
        )
        .with_doc("Bump the resource's modification timestamp.")
    }

    fn malformed_proposal() -> AxiomProposal {
        // Lowercase name + empty doc → fails NameWellFormed + DocNonEmpty
        let mut p = AxiomProposal::new(
            AxiomKind::EnumVariant,
            "some::valid::Path",
            "lowercase_name",
        );
        p.doc = String::new();
        p
    }

    #[test]
    fn primitive_growth_accepts_good_proposal() {
        let mut forge = AxiomForge::new();
        forge.enqueue(good_proposal());
        let results = forge.step();
        assert_eq!(results.len(), 1);
        match &results[0] {
            StepResult::Registered(name) => assert_eq!(name, "TouchTimestamp"),
            other => panic!("expected Registered, got {other:?}"),
        }
        assert_eq!(forge.registry.len(), 1);
    }

    #[test]
    fn primitive_growth_rejects_malformed_proposal() {
        let mut forge = AxiomForge::new();
        forge.enqueue(malformed_proposal());
        let results = forge.step();
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], StepResult::Rejected(_)));
        assert_eq!(forge.registry.len(), 0);
    }

    #[test]
    fn primitive_growth_commits_name_as_id() {
        let mut forge = AxiomForge::new();
        forge.enqueue(good_proposal());
        forge.step();
        let committed_id = forge.registry[0].proposal.name.clone();
        assert_eq!(committed_id, "TouchTimestamp");
    }

    #[test]
    fn primitive_growth_preserves_emission() {
        let mut forge = AxiomForge::new();
        forge.enqueue(good_proposal());
        forge.step();
        let artifact = &forge.registry[0];
        assert!(
            artifact.emission.declaration.contains("TouchTimestamp"),
            "emission should contain the variant name"
        );
        assert_eq!(artifact.frozen_vector.b3sum_hex.len(), 64);
    }

    #[test]
    fn primitive_growth_batched_mix() {
        let mut forge = AxiomForge::new();
        forge.enqueue(good_proposal());
        forge.enqueue(malformed_proposal());
        forge.enqueue(good_proposal());
        let results = forge.step();
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0], StepResult::Registered(_)));
        assert!(matches!(results[1], StepResult::Rejected(_)));
        assert!(matches!(results[2], StepResult::Registered(_)));
        assert_eq!(forge.registry.len(), 2);
    }

    // Debug impl for StepResult so panics show useful info in tests
    impl<Id: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for StepResult<Id, V> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                StepResult::Registered(id) => write!(f, "Registered({id:?})"),
                StepResult::EmittedNone => write!(f, "EmittedNone"),
                StepResult::Rejected(v) => write!(f, "Rejected({v:?})"),
            }
        }
    }
}
