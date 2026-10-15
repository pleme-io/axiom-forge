# axiom-forge

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


**Lisp generates Rust generates Lisp.** The metacircular primitive layer.

## Role in the pleme-io platform

axiom-forge closes the metacircular loop between Lisp (the candidate
generator) and Rust (the axiom space). It takes a typed sexpr proposal
for a new Rust primitive — a new enum variant, a new unit struct — runs
seven structural proofs, and emits Rust source that `rustc` is the final
gatekeeper on.

axiom-forge is specifically **gates 6 + 7** of the ten-gate
forced-realization lattice defined in
`pleme-io/mathscape/docs/arch/machine-synthesis.md`. Gates 1–5 (local
compression/coverage/irreducibility + temporal condensation/
cross-corpus) are the caller's responsibility. This crate is a pure
function: proposal → (Certificate | Violations) → emission. It has no
opinion on reward, does not track history, and does not rewrite
libraries. See `docs/MATHSCAPE_HANDOFF.md` for the bridge protocol.

```
Lisp proposal (sexpr)  ──┐
                         │  verify  (7 obligations)
                         ▼
                    Certificate       ← BLAKE3 content hash = axiom identity
                         │
                         ▼  emit    (string-template codegen)
                    Rust source text  (deterministic)
                         │
                         ▼  rustc    (final gatekeeper)
                    New axiom lives in the workspace
                         │
                         ▼
                    FrozenVector      ← (canonical_text, b3sum) pinned
                                        cross-language portability contract
```

The companion document `iac-forge/docs/METACIRCULAR.md` describes the
full theory. axiom-forge is the executable form.

## The seven proof obligations

An axiom is only emitted if all seven return zero violations. Violations
are collected, never short-circuited — callers see every problem at once.

1. `NameWellFormed`       — PascalCase, ASCII alphanumeric, starts with letter
2. `NameNotReserved`      — not in `DEFAULT_RESERVED` nor caller's extensions
3. `FieldsWellFormed`     — distinct snake_case field names
4. `FieldCountBounded`    — at most `MAX_FIELDS` (= 8)
5. `TargetPathValid`      — `::`-separated path, PascalCase leaf
6. `DocNonEmpty`          — every axiom is documented
7. `ContentAddressable`   — proposal hash is non-zero (audit-trail sanity)

See `src/verify.rs`.

## Why string templates instead of `quote!`

The core emission is intentionally dependency-minimal (no `syn`, no
`quote`, no proc-macro toolchain at the axiom boundary):

- Audit-friendly — easy to inspect emitted output as pure text
- Content-hashable — string bytes go straight into BLAKE3
- Proof-pure — the prover reads sexpr, not syntactic Rust
- Cross-language — other languages can implement the same emission from
  the same sexpr input and hashes will agree

A downstream consumer can layer `syn`/`quote` on top for AST-level work.
The core stays minimal.

## Frozen vectors = cross-language portability contract

Every emitted axiom pairs `(canonical_text, b3sum_hex)`. Any language
implementing canonical sexpr emission + BLAKE3 must produce the same
hash. The portability club grows monotonically — no central coordination
is needed, just agreement on the pair. See `src/vector.rs`.

## Relation to other repos

| Repo | Relation |
|------|----------|
| `iac-forge` | Provides `sexpr::{ToSExpr, FromSExpr, ContentHash}`; axiom-forge is a downstream user |
| `arch-synthesizer` | Typescape consumer — axioms emitted here become leaves in the typescape Merkle tree |
| `substrate-forge` | Runtime for running verified programs; axiom-forge extends *what programs can be* |
| `ml-forge` | Tensor IR axioms (UOps) are the canonical first consumer — proves the model |
| `ruby-synthesizer`, `pangea-forge` | Downstream rendering — axioms minted here can flow through the morphism graph |

## Safety claim

**Every emitted axiom is double-gated.** First: axiom-forge's own
invariant prover refuses to emit source for a proposal that violates
any obligation. Second: even if emission succeeds, `rustc` is the final
gatekeeper — if the generated code does not type-check, the axiom
literally does not exist in the workspace. The pipeline cannot ship
invalid Rust.

## Tests

40 unit tests covering:

- sexpr round-trip for every IR type (AxiomKind, FieldTy, FieldSpec, AxiomProposal, Certificate, FrozenVector)
- each obligation's failure mode in isolation
- multi-violation accumulation (no short-circuit)
- deterministic emission (same input → identical text → identical hash)
- distinct proposals → distinct hashes
- `rustc`-style structural checks on emitted arms

Run: `cargo test`.

## Dependencies

- `blake3`          — content hashing
- `iac-forge`       — sexpr IR + hashing primitives (git dep)
- `thiserror`       — error ergonomics

No `syn`, no `quote`, no proc-macro deps. Deliberate.
