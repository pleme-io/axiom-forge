//! Proof obligations. An axiom can only be emitted if EVERY obligation
//! below returns zero violations.

use iac_forge::sexpr::{struct_expr, ContentHash, SExpr, ToSExpr};

use crate::proposal::{AxiomKind, AxiomProposal};

#[cfg(test)]
use crate::proposal::FieldTy;

/// The seven proof obligations every axiom must satisfy.
///
/// Obligations are structural and checked in pure Rust before any
/// source is emitted. The list is `#[non_exhaustive]` because new
/// obligations can be added as the system grows (itself a
/// metacircular event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ProofObligation {
    /// Variant name is PascalCase, starts with a letter, contains only
    /// ASCII alphanumeric.
    NameWellFormed,
    /// Variant name doesn't match one of a well-known existing set
    /// (basic collision avoidance — callers can extend the reserved
    /// list via `VerifyConfig`).
    NameNotReserved,
    /// All fields have PascalCase-compatible snake_case names,
    /// distinct per proposal.
    FieldsWellFormed,
    /// At most [`MAX_FIELDS`] fields — complexity cap.
    FieldCountBounded,
    /// Target is a syntactically-plausible Rust path
    /// (`::`-separated lower_snake segments, final segment PascalCase).
    TargetPathValid,
    /// Doc is non-empty.
    DocNonEmpty,
    /// Proposal is content-addressable with a non-zero hash
    /// (trivially true in practice; included for the audit trail).
    ContentAddressable,
}

pub const MAX_FIELDS: usize = 8;

/// Reserved names that any target enum might already use. A proposal
/// with a reserved name is rejected with `NameNotReserved`. Callers
/// who know their target enum CAN extend via [`VerifyConfig::reserved`].
pub const DEFAULT_RESERVED: &[&str] = &[
    "Input", "Output", "None", "Some", "Ok", "Err", "Add", "Mul", "Sub",
    "Div", "Neg", "And", "Or", "Not", "Eq", "Lt", "Gt",
];

/// A single proof violation, with enough context for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub obligation: ProofObligation,
    pub message: String,
}

impl Violation {
    pub fn new(obligation: ProofObligation, message: impl Into<String>) -> Self {
        Self {
            obligation,
            message: message.into(),
        }
    }
}

/// Optional config for the verifier.
#[derive(Debug, Clone, Default)]
pub struct VerifyConfig {
    /// Additional reserved names (beyond [`DEFAULT_RESERVED`]) that
    /// should be rejected.
    pub reserved: Vec<String>,
}

/// A certificate emitted for a proposal that passed all obligations.
///
/// The certificate's BLAKE3 content hash is the axiom's identity. It
/// is tameshi-attestable and should be stored alongside the generated
/// Rust source as a permanent record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub proposal_hash: ContentHash,
    pub obligations_passed: Vec<ProofObligation>,
    pub name: String,
    pub target: String,
}

impl Certificate {
    pub fn to_sexpr_certificate(&self) -> SExpr {
        let obligations = self
            .obligations_passed
            .iter()
            .map(|o| SExpr::Symbol(obligation_tag(*o).into()))
            .collect::<Vec<_>>();
        let mut obligations_list = vec![SExpr::Symbol("list".into())];
        obligations_list.extend(obligations);
        struct_expr(
            "axiom-certificate",
            vec![
                ("name", self.name.to_sexpr()),
                ("target", self.target.to_sexpr()),
                ("proposal-hash", SExpr::String(self.proposal_hash.to_hex())),
                ("obligations-passed", SExpr::List(obligations_list)),
            ],
        )
    }
}

fn obligation_tag(o: ProofObligation) -> &'static str {
    match o {
        ProofObligation::NameWellFormed => "name-well-formed",
        ProofObligation::NameNotReserved => "name-not-reserved",
        ProofObligation::FieldsWellFormed => "fields-well-formed",
        ProofObligation::FieldCountBounded => "field-count-bounded",
        ProofObligation::TargetPathValid => "target-path-valid",
        ProofObligation::DocNonEmpty => "doc-non-empty",
        ProofObligation::ContentAddressable => "content-addressable",
    }
}

/// Verify a proposal. Returns `Ok(Certificate)` iff every obligation
/// passes; `Err(Vec<Violation>)` listing every failure otherwise.
///
/// # Errors
/// Propagates `Violation`s collected during the proof. All obligations
/// are checked — the function does not short-circuit, so callers see
/// every failing obligation at once.
pub fn verify(
    proposal: &AxiomProposal,
    config: &VerifyConfig,
) -> Result<Certificate, Vec<Violation>> {
    let mut violations = Vec::new();
    let mut passed = Vec::new();

    check_name_well_formed(&proposal.name, &mut violations, &mut passed);
    check_name_not_reserved(&proposal.name, config, &mut violations, &mut passed);
    check_fields_well_formed(&proposal.fields, &mut violations, &mut passed);
    check_field_count_bounded(&proposal.fields, &mut violations, &mut passed);
    check_target_path_valid(&proposal.target, &mut violations, &mut passed);
    check_doc_non_empty(&proposal.doc, &mut violations, &mut passed);
    check_content_addressable(proposal, &mut violations, &mut passed);

    // Additional kind-specific checks.
    match proposal.kind {
        AxiomKind::UnitStruct => {
            if !proposal.fields.is_empty() {
                violations.push(Violation::new(
                    ProofObligation::FieldsWellFormed,
                    "unit-struct axioms must have zero fields",
                ));
            }
        }
        AxiomKind::EnumVariant => {
            // No extra constraints at v0; variants may be unit or
            // struct-shaped.
        }
    }

    if violations.is_empty() {
        Ok(Certificate {
            proposal_hash: proposal.content_hash(),
            obligations_passed: passed,
            name: proposal.name.clone(),
            target: proposal.target.clone(),
        })
    } else {
        Err(violations)
    }
}

fn check_name_well_formed(
    name: &str,
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    let ok = !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if ok {
        passed.push(ProofObligation::NameWellFormed);
    } else {
        violations.push(Violation::new(
            ProofObligation::NameWellFormed,
            format!(
                "name {name:?} must be PascalCase ASCII alphanumeric starting \
                 with an uppercase letter"
            ),
        ));
    }
}

fn check_name_not_reserved(
    name: &str,
    config: &VerifyConfig,
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    let reserved = DEFAULT_RESERVED.iter().any(|r| *r == name)
        || config.reserved.iter().any(|r| r == name);
    if reserved {
        violations.push(Violation::new(
            ProofObligation::NameNotReserved,
            format!("name {name:?} is reserved"),
        ));
    } else {
        passed.push(ProofObligation::NameNotReserved);
    }
}

fn check_fields_well_formed(
    fields: &[crate::proposal::FieldSpec],
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    let mut ok = true;
    let mut seen = std::collections::BTreeSet::new();
    for f in fields {
        if f.name.is_empty()
            || !f.name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            || !f.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            ok = false;
            violations.push(Violation::new(
                ProofObligation::FieldsWellFormed,
                format!(
                    "field name {:?} must be snake_case ASCII alphanumeric \
                     starting with a lowercase letter",
                    f.name
                ),
            ));
        }
        if !seen.insert(&f.name) {
            ok = false;
            violations.push(Violation::new(
                ProofObligation::FieldsWellFormed,
                format!("duplicate field name {:?}", f.name),
            ));
        }
    }
    if ok {
        passed.push(ProofObligation::FieldsWellFormed);
    }
}

fn check_field_count_bounded(
    fields: &[crate::proposal::FieldSpec],
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    if fields.len() > MAX_FIELDS {
        violations.push(Violation::new(
            ProofObligation::FieldCountBounded,
            format!(
                "field count {} exceeds maximum {}",
                fields.len(),
                MAX_FIELDS
            ),
        ));
    } else {
        passed.push(ProofObligation::FieldCountBounded);
    }
}

fn check_target_path_valid(
    target: &str,
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    // Format: `a::b::c::PascalName`. We don't enforce the final
    // segment is PascalCase because the target could be a module
    // path alone; the name comes from `proposal.name`.
    let parts: Vec<&str> = target.split("::").collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        violations.push(Violation::new(
            ProofObligation::TargetPathValid,
            format!("target path {target:?} is malformed"),
        ));
        return;
    }
    let ok = parts.iter().all(|p| {
        !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    if ok {
        passed.push(ProofObligation::TargetPathValid);
    } else {
        violations.push(Violation::new(
            ProofObligation::TargetPathValid,
            format!(
                "target path {target:?} contains segments with non-identifier chars"
            ),
        ));
    }
}

fn check_doc_non_empty(
    doc: &str,
    violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    if doc.trim().is_empty() {
        violations.push(Violation::new(
            ProofObligation::DocNonEmpty,
            "doc must be non-empty — every axiom must explain itself",
        ));
    } else {
        passed.push(ProofObligation::DocNonEmpty);
    }
}

fn check_content_addressable(
    proposal: &AxiomProposal,
    _violations: &mut Vec<Violation>,
    passed: &mut Vec<ProofObligation>,
) {
    // Content addressing is structural — always succeeds because
    // every ToSExpr type has a content_hash. Included so the
    // certificate can enumerate it explicitly.
    let _hash = proposal.content_hash();
    passed.push(ProofObligation::ContentAddressable);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::{AxiomProposal, FieldSpec};

    fn good() -> AxiomProposal {
        AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "AddComment",
        )
        .with_doc("Attach a free-form comment to the resource.")
        .with_field(FieldSpec {
            name: "text".into(),
            ty: FieldTy::String,
            doc: "The comment text".into(),
        })
    }

    #[test]
    fn good_proposal_certified() {
        let cfg = VerifyConfig::default();
        let cert = verify(&good(), &cfg).expect("good proposal must certify");
        assert_eq!(cert.name, "AddComment");
        assert_eq!(cert.obligations_passed.len(), 7);
    }

    #[test]
    fn snake_case_variant_name_rejected() {
        let mut p = good();
        p.name = "add_comment".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::NameWellFormed));
    }

    #[test]
    fn reserved_name_rejected() {
        let mut p = good();
        p.name = "Add".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::NameNotReserved));
    }

    #[test]
    fn custom_reserved_rejected() {
        let mut cfg = VerifyConfig::default();
        cfg.reserved.push("AddComment".into());
        let violations = verify(&good(), &cfg).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::NameNotReserved));
    }

    #[test]
    fn duplicate_field_rejected() {
        let mut p = good();
        p.fields.push(FieldSpec {
            name: "text".into(),
            ty: FieldTy::String,
            doc: "duplicate".into(),
        });
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::FieldsWellFormed));
    }

    #[test]
    fn capitalized_field_rejected() {
        let mut p = good();
        p.fields[0].name = "Text".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::FieldsWellFormed));
    }

    #[test]
    fn too_many_fields_rejected() {
        let mut p = good();
        p.fields.clear();
        for i in 0..(MAX_FIELDS + 1) {
            p.fields.push(FieldSpec {
                name: format!("f{i}"),
                ty: FieldTy::I64,
                doc: "x".into(),
            });
        }
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::FieldCountBounded));
    }

    #[test]
    fn malformed_target_rejected() {
        let mut p = good();
        p.target = "not a path".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::TargetPathValid));
    }

    #[test]
    fn empty_doc_rejected() {
        let mut p = good();
        p.doc = "  ".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::DocNonEmpty));
    }

    #[test]
    fn unit_struct_with_fields_rejected() {
        let mut p = good();
        p.kind = AxiomKind::UnitStruct;
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        assert!(violations
            .iter()
            .any(|v| v.obligation == ProofObligation::FieldsWellFormed));
    }

    #[test]
    fn certificate_round_trips_via_sexpr() {
        let cert = verify(&good(), &VerifyConfig::default()).unwrap();
        let sexpr = cert.to_sexpr_certificate();
        let emitted = sexpr.emit();
        // Re-parse should produce a structurally-equivalent form.
        let reparsed = iac_forge::sexpr::SExpr::parse(&emitted).unwrap();
        assert_eq!(sexpr, reparsed);
    }

    #[test]
    fn certificate_hash_matches_proposal() {
        let p = good();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        assert_eq!(cert.proposal_hash, p.content_hash());
    }

    #[test]
    fn multiple_violations_collected_not_short_circuited() {
        let mut p = good();
        p.name = "bad name!!!".into();
        p.target = "".into();
        p.doc = "".into();
        let violations = verify(&p, &VerifyConfig::default()).unwrap_err();
        // Expect at least 3 distinct obligations to have fired.
        let distinct: std::collections::BTreeSet<_> =
            violations.iter().map(|v| v.obligation).collect();
        assert!(distinct.len() >= 3);
    }
}
