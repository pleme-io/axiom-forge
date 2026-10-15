//! Frozen cross-language vector — one per accepted axiom.
//!
//! When an axiom lands, its sample canonical emission is paired with
//! the BLAKE3 hash of that emission. The `(canonical_text, b3sum_hex)`
//! pair joins the cross-language portability contract: every language
//! that implements canonical sexpr emission + BLAKE3 must produce the
//! same hash for the sample. This is how the portability club grows
//! monotonically without central coordination.

use iac_forge::sexpr::{
    parse_struct, struct_expr, take_field, FromSExpr, SExpr, SExprError, ToSExpr,
};

use crate::emit::EmissionOutput;
use crate::proposal::AxiomProposal;
use crate::verify::Certificate;

/// A frozen test vector for the portability contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenVector {
    /// Axiom name (PascalCase).
    pub axiom_name: String,
    /// The sample canonical emission (sexpr text).
    pub canonical_text: String,
    /// Lowercase-hex BLAKE3 of `canonical_text` bytes.
    pub b3sum_hex: String,
    /// Proposal content hash (ties this vector back to the axiom's
    /// proposal for audit).
    pub proposal_hash: String,
}

impl FrozenVector {
    /// Build a vector from an emitted axiom. The canonical text is
    /// the `sample_emission` from the emission output.
    #[must_use]
    pub fn from_emission(
        proposal: &AxiomProposal,
        cert: &Certificate,
        emission: &EmissionOutput,
    ) -> Self {
        let canonical_text = emission.sample_emission.clone();
        let hash = blake3::hash(canonical_text.as_bytes());
        Self {
            axiom_name: proposal.name.clone(),
            canonical_text,
            b3sum_hex: hash.to_hex().to_string(),
            proposal_hash: cert.proposal_hash.to_hex(),
        }
    }

    /// Emit the vector as a single Rust test-data line suitable for
    /// pasting into `cross_lang_vectors.rs` style files.
    #[must_use]
    pub fn as_rust_tuple(&self) -> String {
        format!(
            "    (\n        {:?},\n        {:?},\n    ),",
            self.canonical_text, self.b3sum_hex
        )
    }
}

impl ToSExpr for FrozenVector {
    fn to_sexpr(&self) -> SExpr {
        struct_expr(
            "frozen-vector",
            vec![
                ("axiom-name", self.axiom_name.to_sexpr()),
                ("canonical-text", self.canonical_text.to_sexpr()),
                ("b3sum-hex", self.b3sum_hex.to_sexpr()),
                ("proposal-hash", self.proposal_hash.to_sexpr()),
            ],
        )
    }
}

impl FromSExpr for FrozenVector {
    fn from_sexpr(s: &SExpr) -> Result<Self, SExprError> {
        let f = parse_struct(s, "frozen-vector")?;
        Ok(Self {
            axiom_name: String::from_sexpr(take_field(&f, "axiom-name")?)?,
            canonical_text: String::from_sexpr(take_field(&f, "canonical-text")?)?,
            b3sum_hex: String::from_sexpr(take_field(&f, "b3sum-hex")?)?,
            proposal_hash: String::from_sexpr(take_field(&f, "proposal-hash")?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::emit_rust;
    use crate::proposal::{AxiomKind, AxiomProposal, FieldSpec, FieldTy};
    use crate::verify::{verify, VerifyConfig};

    fn setup() -> (AxiomProposal, Certificate, EmissionOutput) {
        let p = AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "TouchTimestamp",
        )
        .with_doc("Bump the resource's modification timestamp.");
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let emission = emit_rust(&p, &cert).unwrap();
        (p, cert, emission)
    }

    #[test]
    fn vector_from_emission_has_all_fields() {
        let (p, cert, emission) = setup();
        let v = FrozenVector::from_emission(&p, &cert, &emission);
        assert_eq!(v.axiom_name, "TouchTimestamp");
        assert_eq!(v.canonical_text, "touch-timestamp");
        assert_eq!(v.b3sum_hex.len(), 64);
        assert_eq!(v.proposal_hash, cert.proposal_hash.to_hex());
    }

    #[test]
    fn vector_hash_agrees_with_independent_blake3() {
        let (p, cert, emission) = setup();
        let v = FrozenVector::from_emission(&p, &cert, &emission);
        let expected = blake3::hash(v.canonical_text.as_bytes()).to_hex().to_string();
        assert_eq!(v.b3sum_hex, expected);
    }

    #[test]
    fn vector_round_trips_via_sexpr() {
        let (p, cert, emission) = setup();
        let v = FrozenVector::from_emission(&p, &cert, &emission);
        let round = FrozenVector::from_sexpr(&v.to_sexpr()).unwrap();
        assert_eq!(round, v);
    }

    #[test]
    fn rust_tuple_format() {
        let (p, cert, emission) = setup();
        let v = FrozenVector::from_emission(&p, &cert, &emission);
        let tup = v.as_rust_tuple();
        assert!(tup.contains("\"touch-timestamp\""));
        assert!(tup.contains(&v.b3sum_hex));
    }

    #[test]
    fn same_emission_same_vector() {
        let (p, cert, emission) = setup();
        let a = FrozenVector::from_emission(&p, &cert, &emission);
        let b = FrozenVector::from_emission(&p, &cert, &emission);
        assert_eq!(a, b);
    }

    #[test]
    fn distinct_emissions_distinct_hashes() {
        let (_, _, emission1) = setup();
        let p2 = AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "UpdateHeader",
        )
        .with_doc("Update the x-header metadata.")
        .with_field(FieldSpec {
            name: "key".into(),
            ty: FieldTy::String,
            doc: "Header key".into(),
        });
        let cert2 = verify(&p2, &VerifyConfig::default()).unwrap();
        let emission2 = emit_rust(&p2, &cert2).unwrap();
        let v1 = FrozenVector::from_emission(
            &AxiomProposal::new(
                AxiomKind::EnumVariant,
                "x::Y",
                "A",
            )
            .with_doc("d"),
            &Certificate {
                proposal_hash: iac_forge::sexpr::ContentHash([0; 32]),
                obligations_passed: vec![],
                name: "A".into(),
                target: "x::Y".into(),
            },
            &emission1,
        );
        let v2 = FrozenVector::from_emission(&p2, &cert2, &emission2);
        assert_ne!(v1.b3sum_hex, v2.b3sum_hex);
    }
}
