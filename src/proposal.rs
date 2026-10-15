//! `AxiomProposal` — a typed proposal for a new Rust enum variant.
//!
//! The proposal is itself a canonical sexpr value. It round-trips
//! through `iac_forge::sexpr::{ToSExpr, FromSExpr}`; it has a BLAKE3
//! content hash; it is the IDENTITY of the proposal for the whole
//! pipeline.

use iac_forge::sexpr::{
    parse_struct, struct_expr, take_field, FromSExpr, SExpr, SExprError, ToSExpr,
};

/// What shape of axiom is being proposed. We start with the two most
/// common kinds; more can be added as Rust PRs (which is itself a
/// metacircular event — extending axiom-forge via axiom-forge would
/// require manual bootstrapping, same as rustc's self-hosting).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AxiomKind {
    /// A new variant of a closed enum (e.g., a new `ResourceOp`,
    /// a new `UOp`, a new `Pattern`). Unit / tuple / struct shapes
    /// are captured via `FieldSpec`.
    EnumVariant,
    /// A new unit struct carrying documented invariants (e.g., a
    /// new `Quality` tag, a new `Capability` bit). Rarer; reserved
    /// for when an enum isn't the right shape.
    UnitStruct,
}

impl AxiomKind {
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::EnumVariant => "enum-variant",
            Self::UnitStruct => "unit-struct",
        }
    }
}

impl ToSExpr for AxiomKind {
    fn to_sexpr(&self) -> SExpr {
        SExpr::Symbol(self.tag().into())
    }
}

impl FromSExpr for AxiomKind {
    fn from_sexpr(s: &SExpr) -> Result<Self, SExprError> {
        match s.as_symbol()? {
            "enum-variant" => Ok(Self::EnumVariant),
            "unit-struct" => Ok(Self::UnitStruct),
            other => Err(SExprError::UnknownVariant(format!("AxiomKind::{other}"))),
        }
    }
}

/// A field in the proposed variant. We support a small whitelist of
/// types — expanding the list is a Rust PR (another metacircular
/// discipline).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FieldTy {
    String,
    I64,
    U64,
    Bool,
    /// A reference to another axiom in the same target enum (encoded
    /// as that enum's type). Useful for recursive variants (e.g.,
    /// `Add(Box<Self>, Box<Self>)`).
    SelfRef,
    /// A `Vec<String>`.
    VecString,
    /// An `Option<String>`.
    OptionString,
}

impl FieldTy {
    #[must_use]
    pub fn rust_type(&self) -> &'static str {
        match self {
            Self::String => "String",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::Bool => "bool",
            Self::SelfRef => "Box<Self>",
            Self::VecString => "Vec<String>",
            Self::OptionString => "Option<String>",
        }
    }

    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::Bool => "bool",
            Self::SelfRef => "self-ref",
            Self::VecString => "vec-string",
            Self::OptionString => "option-string",
        }
    }
}

impl ToSExpr for FieldTy {
    fn to_sexpr(&self) -> SExpr {
        SExpr::Symbol(self.tag().into())
    }
}

impl FromSExpr for FieldTy {
    fn from_sexpr(s: &SExpr) -> Result<Self, SExprError> {
        match s.as_symbol()? {
            "string" => Ok(Self::String),
            "i64" => Ok(Self::I64),
            "u64" => Ok(Self::U64),
            "bool" => Ok(Self::Bool),
            "self-ref" => Ok(Self::SelfRef),
            "vec-string" => Ok(Self::VecString),
            "option-string" => Ok(Self::OptionString),
            other => Err(SExprError::UnknownVariant(format!("FieldTy::{other}"))),
        }
    }
}

/// A named + typed field in the proposed variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldSpec {
    pub name: String,
    pub ty: FieldTy,
    pub doc: String,
}

impl ToSExpr for FieldSpec {
    fn to_sexpr(&self) -> SExpr {
        struct_expr(
            "field",
            vec![
                ("name", self.name.to_sexpr()),
                ("ty", self.ty.to_sexpr()),
                ("doc", self.doc.to_sexpr()),
            ],
        )
    }
}

impl FromSExpr for FieldSpec {
    fn from_sexpr(s: &SExpr) -> Result<Self, SExprError> {
        let f = parse_struct(s, "field")?;
        Ok(Self {
            name: String::from_sexpr(take_field(&f, "name")?)?,
            ty: FieldTy::from_sexpr(take_field(&f, "ty")?)?,
            doc: String::from_sexpr(take_field(&f, "doc")?)?,
        })
    }
}

/// The full proposal: a typed sexpr value with a BLAKE3 content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiomProposal {
    /// What kind of axiom (currently enum-variant or unit-struct).
    pub kind: AxiomKind,
    /// The fully-qualified target (e.g.,
    /// `iac_forge::transform::ops::ResourceOp`). Informational —
    /// axiom-forge doesn't write to that crate directly; it emits
    /// Rust source the caller integrates.
    pub target: String,
    /// The new variant / struct name (PascalCase).
    pub name: String,
    /// Doc string for the new variant.
    pub doc: String,
    /// Zero or more fields.
    pub fields: Vec<FieldSpec>,
    /// Human-readable invariants the axiom asserts (e.g.,
    /// "idempotent", "shape-preserving"). These are informational
    /// in v0; a future version could verify them mechanically.
    pub asserted_invariants: Vec<String>,
}

impl AxiomProposal {
    pub fn new(
        kind: AxiomKind,
        target: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            target: target.into(),
            name: name.into(),
            doc: String::new(),
            fields: Vec::new(),
            asserted_invariants: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = doc.into();
        self
    }

    #[must_use]
    pub fn with_field(mut self, field: FieldSpec) -> Self {
        self.fields.push(field);
        self
    }

    #[must_use]
    pub fn with_invariant(mut self, inv: impl Into<String>) -> Self {
        self.asserted_invariants.push(inv.into());
        self
    }
}

impl ToSExpr for AxiomProposal {
    fn to_sexpr(&self) -> SExpr {
        struct_expr(
            "axiom-proposal",
            vec![
                ("kind", self.kind.to_sexpr()),
                ("target", self.target.to_sexpr()),
                ("name", self.name.to_sexpr()),
                ("doc", self.doc.to_sexpr()),
                ("fields", self.fields.to_sexpr()),
                ("asserted-invariants", self.asserted_invariants.to_sexpr()),
            ],
        )
    }
}

impl FromSExpr for AxiomProposal {
    fn from_sexpr(s: &SExpr) -> Result<Self, SExprError> {
        let f = parse_struct(s, "axiom-proposal")?;
        Ok(Self {
            kind: AxiomKind::from_sexpr(take_field(&f, "kind")?)?,
            target: String::from_sexpr(take_field(&f, "target")?)?,
            name: String::from_sexpr(take_field(&f, "name")?)?,
            doc: String::from_sexpr(take_field(&f, "doc")?)?,
            fields: Vec::<FieldSpec>::from_sexpr(take_field(&f, "fields")?)?,
            asserted_invariants: Vec::<String>::from_sexpr(take_field(
                &f,
                "asserted-invariants",
            )?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proposal() -> AxiomProposal {
        AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "AddComment",
        )
        .with_doc("Attach a free-form comment to a resource's metadata.")
        .with_field(FieldSpec {
            name: "text".into(),
            ty: FieldTy::String,
            doc: "The comment text.".into(),
        })
        .with_invariant("idempotent")
        .with_invariant("no-pii")
    }

    #[test]
    fn proposal_round_trip() {
        let p = sample_proposal();
        let round = AxiomProposal::from_sexpr(&p.to_sexpr()).unwrap();
        assert_eq!(round, p);
    }

    #[test]
    fn proposal_content_hash_deterministic() {
        let p = sample_proposal();
        assert_eq!(p.content_hash(), p.content_hash());
    }

    #[test]
    fn distinct_proposals_distinct_hashes() {
        let a = sample_proposal();
        let b = AxiomProposal::new(
            AxiomKind::EnumVariant,
            a.target.clone(),
            "DifferentName",
        );
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn axiom_kind_round_trip() {
        for k in [AxiomKind::EnumVariant, AxiomKind::UnitStruct] {
            assert_eq!(AxiomKind::from_sexpr(&k.to_sexpr()).unwrap(), k);
        }
    }

    #[test]
    fn field_ty_round_trip_all_variants() {
        for t in [
            FieldTy::String,
            FieldTy::I64,
            FieldTy::U64,
            FieldTy::Bool,
            FieldTy::SelfRef,
            FieldTy::VecString,
            FieldTy::OptionString,
        ] {
            assert_eq!(FieldTy::from_sexpr(&t.to_sexpr()).unwrap(), t);
        }
    }

    #[test]
    fn field_ty_rust_types_correct() {
        assert_eq!(FieldTy::String.rust_type(), "String");
        assert_eq!(FieldTy::SelfRef.rust_type(), "Box<Self>");
        assert_eq!(FieldTy::VecString.rust_type(), "Vec<String>");
    }

    #[test]
    fn field_spec_round_trip() {
        let f = FieldSpec {
            name: "x".into(),
            ty: FieldTy::I64,
            doc: "an integer field".into(),
        };
        assert_eq!(FieldSpec::from_sexpr(&f.to_sexpr()).unwrap(), f);
    }

    #[test]
    fn unknown_kind_rejected() {
        let err =
            AxiomKind::from_sexpr(&SExpr::Symbol("unknown-kind".into())).unwrap_err();
        assert!(matches!(err, SExprError::UnknownVariant(_)));
    }

    #[test]
    fn unknown_field_ty_rejected() {
        let err = FieldTy::from_sexpr(&SExpr::Symbol("quantum".into())).unwrap_err();
        assert!(matches!(err, SExprError::UnknownVariant(_)));
    }

    #[test]
    fn builder_fluent_api() {
        let p = AxiomProposal::new(AxiomKind::EnumVariant, "crate::Foo", "Bar")
            .with_doc("a doc")
            .with_field(FieldSpec {
                name: "x".into(),
                ty: FieldTy::Bool,
                doc: String::new(),
            });
        assert_eq!(p.name, "Bar");
        assert_eq!(p.doc, "a doc");
        assert_eq!(p.fields.len(), 1);
    }
}
