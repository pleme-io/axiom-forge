# Mathscape Handoff — Gates 6 + 7

This document specifies how a mathscape `PromotionSignal` becomes an
`AxiomProposal`, and how axiom-forge's output returns to mathscape as
a `MigrationReport`.

axiom-forge owns **gates 6 + 7** of the ten-gate lattice defined in
`pleme-io/mathscape/docs/arch/machine-synthesis.md`:

- **Gate 6** — the seven structural obligations (`verify::ProofObligation`)
- **Gate 7** — `rustc` typecheck of the emitted source

Everything before gate 6 is mathscape's responsibility. Everything
after gate 7 (library migration, rewrites, deduplication) is also
mathscape's. axiom-forge is a pure function:

```
(PromotionSignal, Artifact) ──► AxiomProposal
AxiomProposal ──► verify() ──► Certificate | Violations
(AxiomProposal, Certificate) ──► emit_rust() ──► EmissionOutput
```

## Signal → proposal map

Mathscape emits a `PromotionSignal` referencing an Artifact. The
bridge (`mathscape-axiom-bridge`) maps the signal to an
`AxiomProposal`:

| mathscape input                             | axiom-forge field                                                                                     |
|---------------------------------------------|-------------------------------------------------------------------------------------------------------|
| `artifact.rule.name`                         | `name` — PascalCased via `meimei::to_pascal_case`                                                      |
| `artifact.rule.lhs` free-variable shape      | `fields` — each free variable in `lhs` becomes a typed `FieldSpec`; type inferred from usage in `rhs`  |
| `signal.rationale`                           | `doc` — human-readable "why this, why now"                                                             |
| `signal.subsumed_hashes`                     | `asserted_invariants` — e.g. `["subsumes: <hash1>, <hash2>"]`                                          |
| `signal.cross_corpus_support`                | `asserted_invariants` (appended) — e.g. `["cross-corpus: arith, egraph, diff"]`                        |
| fixed                                        | `target = "mathscape_core::term::Term"`                                                               |
| fixed                                        | `kind = AxiomKind::EnumVariant`                                                                       |

The map is **total** — any well-formed `PromotionSignal` yields a
structurally valid `AxiomProposal`. *Structurally valid ≠ semantically
accepted* — gates 6–7 still apply and can reject.

## Rejection and rollback

If gate 6 fails (one or more obligations emit violations):

1. axiom-forge returns `Err(Vec<Violation>)`
2. mathscape logs the rejection in the registry as an Artifact of kind
   `PromotionRejection { signal_hash, violations }`
3. the library entry stays at `Axiomatized` status; it may be re-tried
   in a later epoch if conditions change

If gate 7 fails (rustc does not accept the emitted source):

1. axiom-forge's `EmissionOutput` was produced but the caller tooling
   (the bridge) rejects the PR / patch
2. mathscape logs a `PromotionCompileFailure { signal_hash, rustc_error }`
3. same recovery as gate-6 rejection

## MigrationReport shape (axiom-forge side)

When gate 7 passes, axiom-forge returns a **promotion receipt** to
the bridge:

```rust
pub struct PromotionReceipt {
    pub axiom_identity: AxiomIdentity,    // { target, name, proposal_hash }
    pub emission: EmissionOutput,         // the generated Rust source
    pub certificate: Certificate,         // axiom-forge's cert (7 obligations)
    pub frozen_vector: FrozenVector,      // canonical_text + b3sum for portability
}
```

The bridge produces the `MigrationReport` from this receipt plus the
library rewrite computation. axiom-forge itself does not touch the
mathscape registry — the bridge is the single reader/writer across
the boundary.

## Promotion identity chain

Every promoted Rust primitive has a verifiable chain back to the
mathscape event that produced it:

```
   PromotionSignal::content_hash
          ║
          ║  (bridge::signal_to_proposal)
          ▼
   AxiomProposal::content_hash()
          ║
          ║  (verify → Certificate::proposal_hash)
          ▼
   Certificate::proposal_hash
          ║
          ║  (emit_rust → EmissionOutput::content_hash_hex)
          ▼
   EmissionOutput::content_hash_hex
          ║
          ║  (FrozenVector)
          ▼
   (canonical_text, b3sum_hex)
```

Given any of these five hashes, the full chain up and down is
reconstructible from stored Artifacts on both sides of the boundary.
Replay is bytewise deterministic.

## Why axiom-forge stays minimal

axiom-forge does NOT:

- Generate `PromotionSignal`s. That is mathscape's temporal-gate job.
- Compute ΔDL. axiom-forge has no opinion on reward.
- Rewrite the library. That is mathscape's migration job.
- Track cross-corpus history. That is mathscape's registry job.
- Run rustc itself. That is the caller's responsibility
  (CI / operator / mathscape-cli).

axiom-forge's entire surface is: **given a proposal, run 7 obligations
and emit Rust or structured violations.** The pipeline discipline is:
gates 1–5 on the mathscape side, gates 6–7 here, migration back on the
mathscape side.

This minimality is load-bearing. It makes axiom-forge's 40-test suite
tractable, keeps the crate proc-macro-free, and lets the bridge be
swapped with a different generator (e.g., a human, an LLM, another
learning system) without touching this crate's code.

## Extension points

If a future mathscape requires axiom-forge to emit into a different
target enum (e.g. `mathscape_ml::UOp` instead of `Term`), the bridge
is the single edit site:

```rust
pub struct BridgeConfig {
    pub target: String,                   // e.g. "mathscape_core::term::Term"
    pub kind: AxiomKind,                  // e.g. EnumVariant
    pub field_type_inferrer: Arc<dyn FieldTypeInferrer>,
}
```

axiom-forge remains a pure proposal→emission function.

## Relation to the METACIRCULAR loop

This handoff is one instance of the loop documented in
`iac-forge/docs/METACIRCULAR.md`. Mathscape is the first
*non-trivial* Lisp-side generator — it produces proposals from
learned structure rather than hand-typed sexpr files. That
difference is invisible to axiom-forge: the proposal shape is the
same, the obligations are the same, the emission format is the same.
This is the correct level of abstraction.
