//! Emit deterministic Rust source from a certified [`AxiomProposal`].
//!
//! Uses string templates — no syn/quote. Output is:
//!
//! - **Deterministic**: same (proposal, certificate) → byte-identical Rust
//! - **Content-hashable**: the output is itself hashable; the pair
//!   (proposal_hash, emission_hash) forms a complete audit record
//! - **rustc-ready**: the last line of defense; if rustc rejects, the
//!   axiom doesn't exist

use std::fmt::Write;

use crate::proposal::{AxiomKind, AxiomProposal, FieldTy};
use crate::verify::Certificate;

#[derive(Debug, thiserror::Error)]
pub enum EmissionError {
    #[error("emission unsupported for kind {0:?}")]
    UnsupportedKind(AxiomKind),
    #[error("internal formatting error: {0}")]
    Fmt(String),
}

/// The output of a successful emission. Inspectable as pure text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmissionOutput {
    /// The declaration fragment — a Rust enum variant OR unit struct.
    pub declaration: String,
    /// The ToSExpr match arm — paste into the target enum's
    /// `ToSExpr::to_sexpr` body.
    pub to_sexpr_arm: String,
    /// The FromSExpr tag-handling arm — paste into the target enum's
    /// `FromSExpr::from_sexpr` match on tag.
    pub from_sexpr_arm: String,
    /// Doc block ready to go above the variant.
    pub doc_block: String,
    /// Suggested canonical sexpr emission for a sample value
    /// (for frozen cross-language vectors).
    pub sample_emission: String,
}

impl EmissionOutput {
    /// Combine all fragments into a single text blob for inspection /
    /// content-hashing.
    #[must_use]
    pub fn combined(&self) -> String {
        format!(
            "// ── declaration ─────────────────\n{}\
             \n\n// ── doc ─────────────────────────\n{}\
             \n\n// ── ToSExpr arm ────────────────\n{}\
             \n\n// ── FromSExpr arm ──────────────\n{}\
             \n\n// ── sample canonical emission ──\n{}\n",
            self.declaration,
            self.doc_block,
            self.to_sexpr_arm,
            self.from_sexpr_arm,
            self.sample_emission,
        )
    }

    /// BLAKE3 over the combined text. Deterministic, content-addressed.
    #[must_use]
    pub fn content_hash_hex(&self) -> String {
        let h = blake3::hash(self.combined().as_bytes());
        h.to_hex().to_string()
    }
}

/// Emit Rust source for a certified proposal.
///
/// The certificate is required — we can't emit from an unverified
/// proposal. The certificate's `proposal_hash` is stamped in a doc
/// comment so the emitted Rust carries its own audit trail.
pub fn emit_rust(
    proposal: &AxiomProposal,
    cert: &Certificate,
) -> Result<EmissionOutput, EmissionError> {
    match proposal.kind {
        AxiomKind::EnumVariant => emit_enum_variant(proposal, cert),
        AxiomKind::UnitStruct => emit_unit_struct(proposal, cert),
    }
}

fn emit_enum_variant(
    p: &AxiomProposal,
    cert: &Certificate,
) -> Result<EmissionOutput, EmissionError> {
    let doc_block = render_doc(p, cert)?;

    let mut decl = String::new();
    if p.fields.is_empty() {
        writeln!(decl, "{},", p.name).map_err(fmt_err)?;
    } else {
        writeln!(decl, "{} {{", p.name).map_err(fmt_err)?;
        for f in &p.fields {
            writeln!(
                decl,
                "    /// {}\n    {}: {},",
                escape_comment(&f.doc),
                f.name,
                f.ty.rust_type(),
            )
            .map_err(fmt_err)?;
        }
        writeln!(decl, "}},").map_err(fmt_err)?;
    }

    let to_sexpr_arm = render_to_sexpr_arm(p)?;
    let from_sexpr_arm = render_from_sexpr_arm(p)?;
    let sample_emission = render_sample_emission(p)?;

    Ok(EmissionOutput {
        declaration: decl,
        to_sexpr_arm,
        from_sexpr_arm,
        doc_block,
        sample_emission,
    })
}

fn emit_unit_struct(
    p: &AxiomProposal,
    cert: &Certificate,
) -> Result<EmissionOutput, EmissionError> {
    let doc_block = render_doc(p, cert)?;
    let decl = format!(
        "#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]\n\
         pub struct {};\n",
        p.name
    );
    let to_sexpr_arm = format!(
        "impl iac_forge::sexpr::ToSExpr for {} {{\n    \
           fn to_sexpr(&self) -> iac_forge::sexpr::SExpr {{\n        \
             iac_forge::sexpr::SExpr::Symbol(\"{}\".to_string())\n    \
           }}\n\
         }}\n",
        p.name,
        kebab_case(&p.name),
    );
    let from_sexpr_arm = format!(
        "impl iac_forge::sexpr::FromSExpr for {} {{\n    \
           fn from_sexpr(s: &iac_forge::sexpr::SExpr) \
             -> Result<Self, iac_forge::sexpr::SExprError> {{\n        \
             if s.as_symbol()? == \"{}\" {{\n            \
               Ok(Self)\n        \
             }} else {{\n            \
               Err(iac_forge::sexpr::SExprError::UnknownVariant(\n                \
                 format!(\"{}::{{}}\", s.as_symbol()?)\n            \
               ))\n        \
             }}\n    \
           }}\n\
         }}\n",
        p.name,
        kebab_case(&p.name),
        p.name,
    );
    let sample_emission = kebab_case(&p.name);
    Ok(EmissionOutput {
        declaration: decl,
        to_sexpr_arm,
        from_sexpr_arm,
        doc_block,
        sample_emission,
    })
}

fn render_doc(p: &AxiomProposal, cert: &Certificate) -> Result<String, EmissionError> {
    let mut out = String::new();
    for line in p.doc.lines() {
        writeln!(out, "/// {line}").map_err(fmt_err)?;
    }
    writeln!(out, "///").map_err(fmt_err)?;
    writeln!(
        out,
        "/// Axiom introduced by axiom-forge. Certificate:\n\
         /// - proposal-hash: {}\n\
         /// - obligations passed: {}",
        cert.proposal_hash,
        cert.obligations_passed.len(),
    )
    .map_err(fmt_err)?;
    if !p.asserted_invariants.is_empty() {
        writeln!(out, "///\n/// Asserted invariants:").map_err(fmt_err)?;
        for inv in &p.asserted_invariants {
            writeln!(out, "/// - {inv}").map_err(fmt_err)?;
        }
    }
    Ok(out)
}

fn render_to_sexpr_arm(p: &AxiomProposal) -> Result<String, EmissionError> {
    let tag = kebab_case(&p.name);
    let mut out = String::new();
    if p.fields.is_empty() {
        writeln!(
            out,
            "Self::{} => iac_forge::sexpr::SExpr::Symbol(\"{}\".to_string()),",
            p.name, tag,
        )
        .map_err(fmt_err)?;
        return Ok(out);
    }
    let field_names: Vec<&str> = p.fields.iter().map(|f| f.name.as_str()).collect();
    writeln!(out, "Self::{} {{ {} }} => {{", p.name, field_names.join(", "))
        .map_err(fmt_err)?;
    writeln!(
        out,
        "    let mut items = vec![iac_forge::sexpr::SExpr::Symbol(\"{tag}\".into())];"
    )
    .map_err(fmt_err)?;
    for f in &p.fields {
        match f.ty {
            FieldTy::String => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::SExpr::String({}.clone()));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::I64 => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::SExpr::Integer(*{}));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::U64 => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::SExpr::Integer(*{} as i64));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::Bool => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::SExpr::Bool(*{}));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::SelfRef => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::ToSExpr::to_sexpr(&**{}));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::VecString => {
                writeln!(
                    out,
                    "    items.push(iac_forge::sexpr::ToSExpr::to_sexpr({}));",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::OptionString => {
                writeln!(
                    out,
                    "    items.push(match {} {{\
                     \n        Some(s) => iac_forge::sexpr::SExpr::String(s.clone()),\
                     \n        None => iac_forge::sexpr::SExpr::Nil,\
                     \n    }});",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
        }
    }
    writeln!(out, "    iac_forge::sexpr::SExpr::List(items)").map_err(fmt_err)?;
    writeln!(out, "}},").map_err(fmt_err)?;
    Ok(out)
}

fn render_from_sexpr_arm(p: &AxiomProposal) -> Result<String, EmissionError> {
    let tag = kebab_case(&p.name);
    let mut out = String::new();
    if p.fields.is_empty() {
        writeln!(out, "\"{tag}\" => Ok(Self::{}),", p.name).map_err(fmt_err)?;
        return Ok(out);
    }
    writeln!(out, "\"{tag}\" => {{").map_err(fmt_err)?;
    writeln!(
        out,
        "    if rest.len() != {} {{\
         \n        return Err(iac_forge::sexpr::SExprError::Shape(\
         \n            format!(\"{} expects {} args, got {{}}\", rest.len())\
         \n        ));\
         \n    }}",
        p.fields.len(),
        tag,
        p.fields.len(),
    )
    .map_err(fmt_err)?;
    for (i, f) in p.fields.iter().enumerate() {
        match f.ty {
            FieldTy::String => {
                writeln!(
                    out,
                    "    let {} = String::from_sexpr(&rest[{i}])?;",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::I64 => {
                writeln!(
                    out,
                    "    let {} = match &rest[{i}] {{\
                     \n        iac_forge::sexpr::SExpr::Integer(n) => *n,\
                     \n        other => return Err(\
                     iac_forge::sexpr::SExprError::Shape(\
                     format!(\"expected i64 at field {}, got {{other:?}}\"))),\
                     \n    }};",
                    f.name, f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::U64 => {
                writeln!(
                    out,
                    "    let {} = match &rest[{i}] {{\
                     \n        iac_forge::sexpr::SExpr::Integer(n) if *n >= 0 => *n as u64,\
                     \n        other => return Err(\
                     iac_forge::sexpr::SExprError::Shape(\
                     format!(\"expected non-negative integer at field {}, got {{other:?}}\"))),\
                     \n    }};",
                    f.name, f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::Bool => {
                writeln!(
                    out,
                    "    let {} = match &rest[{i}] {{\
                     \n        iac_forge::sexpr::SExpr::Bool(b) => *b,\
                     \n        other => return Err(\
                     iac_forge::sexpr::SExprError::Shape(\
                     format!(\"expected bool at field {}, got {{other:?}}\"))),\
                     \n    }};",
                    f.name, f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::SelfRef => {
                writeln!(
                    out,
                    "    let {} = Box::new(Self::from_sexpr(&rest[{i}])?);",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::VecString => {
                writeln!(
                    out,
                    "    let {} = Vec::<String>::from_sexpr(&rest[{i}])?;",
                    f.name,
                )
                .map_err(fmt_err)?;
            }
            FieldTy::OptionString => {
                writeln!(
                    out,
                    "    let {} = match &rest[{i}] {{\
                     \n        iac_forge::sexpr::SExpr::Nil => None,\
                     \n        iac_forge::sexpr::SExpr::String(s) => Some(s.clone()),\
                     \n        other => return Err(\
                     iac_forge::sexpr::SExprError::Shape(\
                     format!(\"expected string or nil at field {}, got {{other:?}}\"))),\
                     \n    }};",
                    f.name, f.name,
                )
                .map_err(fmt_err)?;
            }
        }
    }
    write!(out, "    Ok(Self::{} {{", p.name).map_err(fmt_err)?;
    let field_names: Vec<&str> = p.fields.iter().map(|f| f.name.as_str()).collect();
    for (i, n) in field_names.iter().enumerate() {
        if i > 0 {
            write!(out, ",").map_err(fmt_err)?;
        }
        write!(out, " {n}").map_err(fmt_err)?;
    }
    writeln!(out, " }})").map_err(fmt_err)?;
    writeln!(out, "}},").map_err(fmt_err)?;
    Ok(out)
}

fn render_sample_emission(p: &AxiomProposal) -> Result<String, EmissionError> {
    let tag = kebab_case(&p.name);
    if p.fields.is_empty() {
        return Ok(tag);
    }
    let mut out = format!("({tag}");
    for f in &p.fields {
        let sample = match f.ty {
            FieldTy::String => format!(" \"{}\"", f.name),
            FieldTy::I64 => " 0".to_string(),
            FieldTy::U64 => " 0".to_string(),
            FieldTy::Bool => " false".to_string(),
            FieldTy::SelfRef => " <self-ref>".to_string(),
            FieldTy::VecString => " (list)".to_string(),
            FieldTy::OptionString => " nil".to_string(),
        };
        out.push_str(&sample);
    }
    out.push(')');
    Ok(out)
}

/// `PascalCase` → `kebab-case`, the convention for canonical sexpr tags.
fn kebab_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_comment(s: &str) -> String {
    s.replace('\n', " ")
}

fn fmt_err(e: std::fmt::Error) -> EmissionError {
    EmissionError::Fmt(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::FieldSpec;
    use crate::verify::{verify, VerifyConfig};

    fn proposal_with_fields() -> AxiomProposal {
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
        .with_field(FieldSpec {
            name: "pinned".into(),
            ty: FieldTy::Bool,
            doc: "Whether the comment is pinned".into(),
        })
    }

    fn unit_variant() -> AxiomProposal {
        AxiomProposal::new(
            AxiomKind::EnumVariant,
            "iac_forge::transform::ops::ResourceOp",
            "TouchTimestamp",
        )
        .with_doc("Bump the resource's modification timestamp.")
    }

    fn unit_struct() -> AxiomProposal {
        AxiomProposal::new(
            AxiomKind::UnitStruct,
            "iac_forge::capability",
            "ReadOnlyToken",
        )
        .with_doc("A capability token granting read-only access.")
    }

    #[test]
    fn emit_enum_variant_with_fields() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.declaration.contains("AddComment {"));
        assert!(out.declaration.contains("text: String,"));
        assert!(out.declaration.contains("pinned: bool,"));
        assert!(out.to_sexpr_arm.contains("Self::AddComment { text, pinned }"));
        assert!(out.from_sexpr_arm.contains("\"add-comment\""));
        assert!(out.sample_emission.starts_with("(add-comment"));
    }

    #[test]
    fn emit_enum_variant_unit() {
        let p = unit_variant();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.declaration.contains("TouchTimestamp,"));
        assert!(out.to_sexpr_arm.contains("Self::TouchTimestamp"));
        assert!(out.sample_emission == "touch-timestamp");
    }

    #[test]
    fn emit_unit_struct() {
        let p = unit_struct();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.declaration.contains("pub struct ReadOnlyToken;"));
        assert!(out.to_sexpr_arm.contains("impl iac_forge::sexpr::ToSExpr"));
        assert!(out.from_sexpr_arm.contains("impl iac_forge::sexpr::FromSExpr"));
    }

    #[test]
    fn doc_includes_proposal_hash() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.doc_block.contains("proposal-hash:"));
        assert!(out.doc_block.contains(&cert.proposal_hash.to_hex()));
    }

    #[test]
    fn doc_enumerates_asserted_invariants() {
        let mut p = proposal_with_fields();
        p.asserted_invariants.push("idempotent".into());
        p.asserted_invariants.push("pii-free".into());
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.doc_block.contains("- idempotent"));
        assert!(out.doc_block.contains("- pii-free"));
    }

    #[test]
    fn emission_is_deterministic() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let a = emit_rust(&p, &cert).unwrap();
        let b = emit_rust(&p, &cert).unwrap();
        assert_eq!(a.combined(), b.combined());
        assert_eq!(a.content_hash_hex(), b.content_hash_hex());
    }

    #[test]
    fn distinct_proposals_distinct_emission_hashes() {
        let a = proposal_with_fields();
        let b = unit_variant();
        let ca = verify(&a, &VerifyConfig::default()).unwrap();
        let cb = verify(&b, &VerifyConfig::default()).unwrap();
        let ea = emit_rust(&a, &ca).unwrap();
        let eb = emit_rust(&b, &cb).unwrap();
        assert_ne!(ea.content_hash_hex(), eb.content_hash_hex());
    }

    #[test]
    fn combined_contains_every_section_marker() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        let combined = out.combined();
        assert!(combined.contains("declaration"));
        assert!(combined.contains("doc"));
        assert!(combined.contains("ToSExpr arm"));
        assert!(combined.contains("FromSExpr arm"));
        assert!(combined.contains("sample canonical emission"));
    }

    #[test]
    fn kebab_case_conversion() {
        assert_eq!(kebab_case("AddComment"), "add-comment");
        assert_eq!(kebab_case("MarkSensitive"), "mark-sensitive");
        assert_eq!(kebab_case("A"), "a");
        assert_eq!(kebab_case("XMLHttpRequest"), "x-m-l-http-request");
    }

    #[test]
    fn content_hash_hex_is_64_chars() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert_eq!(out.content_hash_hex().len(), 64);
    }

    #[test]
    fn from_sexpr_arm_has_arity_check() {
        let p = proposal_with_fields();
        let cert = verify(&p, &VerifyConfig::default()).unwrap();
        let out = emit_rust(&p, &cert).unwrap();
        assert!(out.from_sexpr_arm.contains("rest.len() != 2"));
    }
}
