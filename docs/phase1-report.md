# FastDB Phase 1 Report

## Proceed to Phase 2

Phase 1 satisfies the parser, independent AST, limits, diagnostics, frontend capability-gate, compatibility-matrix, and local fuzz gates. The Tracy Phase 0 vertical slice still executes through direct Turso AST lowering, while every broader successfully parsed Phase 1 form stops at a spanned frontend `UnsupportedSyntax` error. No Phase 2 catalog/schema behavior or Phase 3 general CRUD/transaction execution was added.

## Inputs and phase state

- Active contract: `revised_plan.md` plus the authoritative `plan-phase1.md` dated 2026-08-12.
- Turso engine pin: `977383ff40edc44ef410af062ed0d2322252a869`; no pin change.
- SurrealDB behavioral reference: official unmodified `v3.1.5` Linux x86-64 binary plus public documentation only.
- Reference asset checksum: `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- Rust: stable `rustc 1.88.0 (6b00bc388 2025-06-23)`; fuzz toolchain `rustc 1.99.0-nightly (3d6c19bb9 2026-08-11)`; `cargo-fuzz 0.13.2`.
- `AGENTS.md` identifies Phase 1 and `plan-phase1.md` as authoritative. `docs/phase0-report.md` remains completed evidence.

The record-ID characterization gate initially found ambiguous plan wording: v3.1.5 rejects `person:'quoted id'` but accepts ``person:`quoted id` ``. Design review on 2026-08-12 selected the reference-compatible form. The live plans now say “backtick-quoted UTF-8 text”; single and double quotes remain value-string delimiters. `docs/compat-research/phase1.md` records the exact observation and decision.

## GitHub Actions state

The pre-Phase 1 setting was Actions enabled, all actions allowed, SHA pinning not required. The user disabled Actions through repository settings. Final verification returned:

```json
{"enabled":false,"sha_pinning_required":false}
```

All verification was local. The 36 inherited workflow files remain in place and unchanged. Re-enabling Actions is outside Phase 1.

## Tracy rename and baseline evidence

Every FastDB-authored Phase 0 source, test, benchmark, plan, report, research note, and review now uses `person:tracy`, `Tracy`, and canonical RID `s:5:tracy`. A case-insensitive scan of every changed or added file for the former identity returned no match; inherited upstream content was not changed for this rename.

Before parser expansion, the renamed Phase 0 suite passed all 66 tests: 30 parser, 15 frontend, and 21 integration tests. This established that the rename was behavior-neutral. The final expanded suite passes 67 tests: 28 parser/contract tests, the unchanged 15 frontend tests, and 24 integration tests including three Phase 1 capability-boundary tests. Phase 0 parser shapes, spans, legacy string escaping, malformed input behavior, all four executable shapes, atomicity, reopen, index selection, version refusal, and error categories remain covered.

## Lexer, AST, and parser API

`turso_fastdb_parser` is now a linear UTF-8 lexer over `&str` with byte spans and no duplicate character/offset buffers. It handles case-insensitive keywords, original identifier text, Unicode identifiers/strings, all four comment forms, characterized single/double string escapes, backtick-quoted record text, decimal numbers, parameters, punctuation/operators, and explicit unsupported introducers. Semicolons in strings or comments never split statements.

The engine-independent AST contains ordered `Script` statements and all required statement variants: `Create`, `Select`, `Update`, `Delete`, `DefineTable`, `DefineField`, `DefineIndex`, `Begin`, `Commit`, and `Cancel`. It distinguishes table targets from typed record IDs; represents omitted CREATE IDs through the table target; preserves bare, backtick-quoted, and signed-`i64` components; models all MVP values, expressions, paths, schema types, clauses, and recursive `option<T>`; and retains source order for scripts and object fields. Statements, targets, identifiers, record components, paths/segments, literals, object keys/fields, types, clauses, and operators carry byte spans. The parser crate contains no `turso_core` or `turso_parser` types.

The internal interfaces are:

```rust
parse(input: &str) -> Result<Script, ParseError>
parse_with_limits(input: &str, limits: &ParserLimits) -> Result<Script, ParseError>
parse_one(input: &str) -> Result<Statement, ParseError>
parse_one_with_limits(input: &str, limits: &ParserLimits) -> Result<Statement, ParseError>
```

Empty/comment-only input, empty statements, missing statement separators, and multiple statements through `parse_one` fail explicitly. One trailing semicolon is allowed. AST `Debug` output is exercised by tests/fuzzing and is never used for Turso SQL generation.

## Grammar coverage

The Pratt parser implements left-associative unary, multiplicative, additive, relational, equality, `AND`, then `OR` binding in the fixed order. `P1-LEX-*`, `P1-EXPR-*`, and `P1-STMT-*` tests cover every required literal, comment/string form, record-ID component, path, nested collection, parameter, precedence level, statement shape, clause option, separator rule, clause order/duplication error, and transaction/schema form.

Functions, casts, subqueries, traversal, ranges, indexing, modulo/power, symbolic boolean aliases, broader comparisons, excluded statement families, and excluded clauses are never silently accepted. Recognized introducers such as `INSERT`, `UPSERT`, `RELATE`, `LET`, `FETCH`, `GROUP`, `TIMEOUT`, `PARALLEL`, `MERGE`, advanced index kinds, and transaction suffixes produce `fastdb::parse::unsupported_syntax` at the introducer.

The clean-room note records the intentional fixed-MVP divergences from observed v3.1.5 behavior: FastDB keyword `NOT`, finite-float rejection, bare-only transaction spellings, syntax-AST preservation of duplicate object keys, and the legacy doubled-single-quote Phase 0 seam. None is represented as complete compatibility.

## Limits and diagnostics

`ParserLimits::default()` enforces:

| Resource | Default |
| --- | ---: |
| Input | 1 MiB |
| Tokens | 65,536 |
| Nesting depth | 64 |
| Elements per comma-delimited collection | 1,024 |
| Statements per script | 256 |
| Identifier/parameter length | 256 UTF-8 bytes |

`P1-LIMIT-001` through `P1-LIMIT-006` test below, exactly at, and above every default. Parentheses, unary expressions, arrays, objects, and recursive generic types all consume the same depth budget. Token/input limits bound lexing allocation, and collection/statement limits bound parser vectors.

Typed `miette` errors cover empty input; unexpected characters, tokens, and EOF; unterminated strings, quoted identifiers, and comments; invalid escapes/numbers; statement separators; empty/multiple statements; duplicate and out-of-order clauses; invalid combinations; unsupported syntax; and every limit category. Each has a deterministic `fastdb::parse::*` code and a smallest-useful byte span. `P1-DIAG-001` fixes colorless 60-column rendering, code, message, label, and span; malformed regression and fuzz cases ensure diagnostics do not expose internal SQL.

## Frontend capability boundary

`Connection::execute` parses exclusively through `parse_one`. A source-aware capability gate accepts only:

```text
CREATE table:bare SET field = 'string'
SELECT * FROM table:bare
SELECT * FROM table WHERE field = 'string'
DELETE table:bare
```

The existing direct AST lowering, bound user values, transaction ownership, decoding, and storage behavior remain unchanged behind that gate. Double-quoted strings, broader values/paths/clauses, quoted or numeric IDs, all UPDATE/schema/transaction statements, and every other parsed Phase 1 shape return frontend `UnsupportedSyntax` with a source span. Malformed source remains category `Parse`; `P1-BRIDGE-001` through `P1-BRIDGE-005` verify execution, gating, parser regressions, and category separation.

## Compatibility contract

`COMPAT.md` now has 45 feature-level rows spanning values, record IDs, expressions, paths/operators, statements, clauses, schema types, transactions, and exclusions. Every row has a stable ID, exact syntax boundary, a real Phase 1 parser test ID, a link into the allowed-source Phase 1 provenance note, and an execution/conformance disposition. `P1-COMPAT-001` mechanically verifies the table shape, referenced test functions, provenance links, allowed statuses, and that parser-only rows are not mislabeled executable.

The executable Phase 0 subset remains `Partial`; broader parser-only forms remain `Planned`; explicit exclusions are `Unsupported`. There are no Phase 1 `Supported` rows.

## Fuzz results

`fastdb-parser/fuzz/` is a detached cargo-fuzz package with an independently authored `parse` target. Arbitrary bytes are safely filtered to valid UTF-8 before invoking the `&str` API. Successful parses recursively validate that every nested span is within its parent/source and that `Debug` is safe. Five authored seeds cover all ten statement variants and malformed lexical/nesting boundaries. Hash-named runtime discoveries and artifacts are ignored and were moved to temporary storage; they are not committed as provenance material.

Final required command:

```text
cargo +nightly fuzz run parse -- -max_total_time=300
Done 10212020 runs in 301 second(s)
```

No crash, assertion failure, sanitizer finding, or hang occurred. An earlier complete five-minute run also finished cleanly with 8,843,418 executions; the final result above is the acceptance evidence for the final target/parser.

## Local verification

All required commands passed on 2026-08-12:

```text
cargo metadata --no-deps --format-version 1
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test -p turso_fastdb_parser                 # 28 passed
cargo test -p turso_fastdb                        # 15 passed
cargo test -p turso_fastdb_tests                  # 24 passed
cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run
cargo +nightly fuzz run parse -- -max_total_time=300
git diff --check
```

Clippy/tests/benchmark compilation report only the inherited Turso core unused-import and unfulfilled-lint-expectation warnings; Phase 1 did not modify those files.

## Changed-file audit

- FastDB parser/AST/diagnostics, frontend seam, Phase 1 Rust tests, detached fuzz package/seeds, compatibility matrix/research/report, active plan/agent state, fuzz ignores, and Tracy-authored Phase 0 evidence/examples changed.
- No file under `core/`, `sqlite/parser/`, `bindings/`, `postgres/`, inherited upstream test trees, WAL/JSONB/optimizer code, or `.github/workflows/` changed.
- No generated SQLite/Turso SQL is present in a parser diagnostic or derived from AST `Debug` output.
- The repository remains one monorepo; the Turso pin, root workspace membership, and root lockfile did not change.

## Remaining risks and Phase 2 boundary

- Phase 1 establishes an internal syntax/AST contract, not a public API or stable file format. It may evolve until Phase 4.
- The clean-room probes are representative characterization of the fixed MVP boundaries, not complete SurrealQL conformance. Intentional divergences are documented in both the research note and matrix.
- Duplicate object-key execution semantics, generated UUIDv7 IDs, stable catalogs, schema enforcement, public DEFINE behavior, general CRUD/parameters, and explicit transaction semantics remain for their assigned phases.
- The frontend gate is deliberately narrow; parser acceptance must not be mistaken for executable support.
- GitHub Actions remain disabled, so Phase 2 must continue local verification unless a later explicit decision re-enables them.

No Phase 1 stop condition remains. Proceed to Phase 2 without broadening this result into production-readiness, complete compatibility, cloud readiness, or release-quality performance claims.
