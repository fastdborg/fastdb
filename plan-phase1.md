# FastDB Phase 1 — Parser, AST, and Compatibility Contract

Status: authoritative execution plan, 2026-08-12

## 1. Objective and Boundary

Phase 1 expands the independent FastDB parser from the passing 66-test Phase 0 baseline into the complete MVP syntax and AST contract while preserving the existing vertical slice.

Phase 1 does not implement general CRUD execution, stable catalogs, schema enforcement, public APIs, or CLI behavior. Newly parsed features remain `Planned` in `COMPAT.md` and receive explicit frontend `UnsupportedSyntax` errors until their execution phase.

No Turso core, SQLite parser, WAL, JSONB, optimizer, or inherited workflow file may change.

## 2. Execution Sequence

### P1.0 — Disable GitHub Actions

1. Record the existing repository setting: Actions enabled, all actions allowed, SHA pinning not required.
2. Disable Actions for `fastdborg/fastdb` through the repository-level Actions permission setting:

   ```sh
   gh api --method PUT repos/fastdborg/fastdb/actions/permissions -F enabled=false
   ```

3. Verify `gh api repos/fastdborg/fastdb/actions/permissions` reports `"enabled": false`.
4. Do not edit or relocate the inherited workflow files.
5. Keep all Phase 1 verification local. Re-enabling Actions is outside Phase 1 and requires an explicit later decision.

If repository policy or permissions prevent disabling Actions, stop before implementation and request administrator action.

### P1.1 — Phase transition and demo rename

1. Update `AGENTS.md` so Phase 1 and this plan are authoritative; retain `docs/phase0-report.md` as completed evidence.
2. Replace the legacy Phase 0 demo identity with `person:tracy` and `"Tracy"` in every FastDB-authored source, test, benchmark, plan, report, research note, and review file.
3. Update canonical RID expectations to `s:5:tracy`, malformed-RID fixtures consistently, and all documentation examples.
4. Preserve inherited Turso content; do not edit upstream files merely because they contain the legacy name.
5. Verify no FastDB-authored changed or added file contains the legacy identity.
6. Rerun the complete Phase 0 FastDB suite before parser expansion so the rename is proven behavior-neutral.

### P1.2 — Clean-room grammar characterization

Create `docs/compat-research/phase1.md` using only public documentation and independently designed black-box queries against an unmodified SurrealDB `v3.1.5` binary.

Record exact source, version, date, input, and output for:

- Four comment forms and unterminated/nested block-comment behavior.
- Single- and double-quoted strings, escapes, Unicode, and multiline content.
- Identifier and keyword case behavior.
- Integer/float grammar, exponents, overflow, and non-finite rejection.
- Bare, backtick-quoted text, and signed-integer record IDs.
- Arrays, objects, optional trailing commas, and duplicate object keys.
- Operator precedence and associativity.
- `ONLY`, `RETURN`, `ORDER BY`, `LIMIT`, `START`, schema statements, and transaction spellings.
- Recognized but excluded clauses requiring precise unsupported errors.

Download the official binary into temporary storage, verify it reports `3.1.5`, record its release URL/checksum and local environment, and do not commit the binary or any SurrealDB tests or fixtures. If the fixed MVP contract requires syntax rejected by `v3.1.5`, stop and document the contradiction rather than silently changing `revised_plan.md`.

### P1.3 — Lexer, limits, and diagnostics

Refactor `turso_fastdb_parser` into a linear, byte-spanned lexer over `&str`; avoid duplicate character and offset buffers.

Support case-insensitive keywords with original identifier text preserved; Unicode strings and identifiers; `#`, `//`, `--`, and `/* … */` comments; single- and double-quoted strings using the characterized escape rules; decimal integers/floats; parameters; punctuation and supported operators; semicolons inside strings/comments; and explicit tokens for unsupported statement/clause introducers.

Introduce `ParserLimits` with these defaults:

- Input: 1 MiB.
- Tokens: 65,536.
- Nesting depth: 64.
- Elements in any comma-delimited collection: 1,024.
- Statements per script: 256.
- Identifier/parameter length: 256 UTF-8 bytes.

Every recursive parser path—parentheses, unary expressions, arrays, objects, and generic types—must consume a depth budget. Test each limit below, exactly at, and above its boundary.

Expand typed `miette` diagnostics for empty input, unexpected character/token/EOF, unterminated strings/comments, invalid escapes/numbers, missing separators, duplicate or out-of-order clauses, unsupported syntax, and every limit category. Each diagnostic carries the smallest useful byte span, a deterministic diagnostic code, and no generated internal SQL.

### P1.4 — Independent AST and parser interfaces

The parser crate must contain no `turso_core` or `turso_parser` types. It exposes:

```rust
parse(input: &str) -> Result<Script, ParseError>
parse_with_limits(input: &str, limits: &ParserLimits) -> Result<Script, ParseError>
parse_one(input: &str) -> Result<Statement, ParseError>
parse_one_with_limits(input: &str, limits: &ParserLimits) -> Result<Statement, ParseError>
```

`Script` contains ordered statements and an enclosing span. Empty or comment-only input is an error; one optional trailing semicolon is accepted; empty statements and missing separators are rejected. `parse_one` rejects zero or multiple statements and remains the Phase 0 frontend bridge.

AST requirements:

- `Statement`: `Create`, `Select`, `Update`, `Delete`, `DefineTable`, `DefineField`, `DefineIndex`, `Begin`, `Commit`, and `Cancel`.
- Targets distinguish logical tables from typed record IDs.
- Record-ID components support bare UTF-8 text, backtick-quoted UTF-8 text, and signed `i64`; omitted CREATE IDs are explicit.
- `Expr` supports null, bool, validated integer/finite-float literals, strings, arrays, objects, parameters, record IDs, field paths, unary operators, binary operators, and parentheses.
- Field paths preserve each segment and its span.
- Schema types cover `bool`, `int`, `float`, `number`, `string`, `object`, `array`, `record`, and recursive `option<T>`.
- All nodes, identifiers, literals, and operators carry byte spans.
- Object fields and statement lists preserve source order.
- AST/debug output must never be used as Turso SQL generation.

The primary parser API is internal and may evolve until the Phase 4 public API freeze.

### P1.5 — Pratt expressions and exact MVP grammar

Implement a Pratt parser with left-associative binary operators and this highest-to-lowest precedence, subject to recorded `v3.1.5` observations:

1. Unary `NOT`, `+`, `-`.
2. Multiplicative `*`, `/`.
3. Additive `+`, `-`.
4. Relational `<`, `<=`, `>`, `>=`.
5. Equality `=`, `!=`.
6. `AND`.
7. `OR`.

Do not accept functions, casts, subqueries, graph traversal, ranges, array indexing, power/modulo, symbolic boolean aliases, or broader comparison operators.

Implement these exact shapes:

```text
CREATE [ONLY] table[:id]
  (CONTENT expression | SET path = expression [, ...])
  [RETURN AFTER | RETURN NONE | RETURN BEFORE]

SELECT (* | path [AS alias] [, ...])
  FROM [ONLY] (table | table:id)
  [WHERE expression]
  [ORDER BY path [ASC | DESC] [, ...]]
  [LIMIT nonnegative-integer]
  [START nonnegative-integer]

UPDATE (table | table:id)
  SET path = expression [, ...]
  [WHERE expression]
  [RETURN AFTER | RETURN NONE]

DELETE (table | table:id)
  [WHERE expression]
  [RETURN BEFORE]

DEFINE TABLE name (SCHEMALESS | SCHEMAFULL)

DEFINE FIELD path ON [TABLE] name TYPE type

DEFINE INDEX name ON [TABLE] name
  FIELDS path [, ...] [UNIQUE]

BEGIN
COMMIT
CANCEL
```

Additional rules:

- `CONTENT` and `SET` are mutually exclusive.
- Projection `*` cannot be mixed with named projections.
- `ONLY` is retained in the AST; structural validity is checked without execution.
- `LIMIT` and `START` must fit the documented nonnegative integer range.
- Clause order is fixed; duplicates or trailing clauses are never ignored.
- Unsupported statement families and excluded MVP clauses produce `UnsupportedSyntax` at their introducing token.
- Multiple statements are returned in source order.

### P1.6 — Preserve the Phase 0 execution seam

1. Continue parsing `Connection::execute` through `parse_one`.
2. Add a capability gate accepting only the four Tracy-based Phase 0 shapes.
3. Lower and execute those shapes exactly as before.
4. Return a spanned frontend `UnsupportedSyntax` error for every other successfully parsed Phase 1 AST.
5. Keep syntax errors categorized as `Parse`; do not disguise capability-gate failures as parser failures.
6. Do not implement Phase 2 catalogs/schema or Phase 3 general CRUD/transactions.

### P1.7 — Compatibility contract and report

Expand `COMPAT.md` into feature-level rows covering values, record IDs, expressions, statements, clauses, schema types, transactions, and explicit exclusions. Each row contains a stable feature ID, overall status, exact accepted or rejected syntax, Phase 1 parser test IDs, a provenance-note link, and an execution/conformance test ID when one exists.

Parser acceptance alone does not make a feature `Supported` or `Partial`. General Phase 1 forms remain `Planned` until frontend execution exists; the executable Phase 0 subset remains `Partial`.

Create `docs/phase1-report.md` leading with `Proceed to Phase 2` or `Stop for design review`, followed by inputs, Actions state, renamed demo evidence, AST/API summary, grammar coverage, limits, diagnostics, fuzz results, compatibility mapping, local commands, changed-file audit, and unresolved risks.

## 3. Test and Acceptance Plan

Use Rust unit/integration tests, deterministic rendered-diagnostic expectations, and cargo-fuzz; do not introduce a new SQL test format.

| Test group | Required scenarios |
| --- | --- |
| `P1-LEX-*` | Every token/comment/string form; Unicode byte spans; keyword casing; invalid characters/escapes/numbers; unterminated literals/comments |
| `P1-EXPR-*` | Every literal; nested arrays/objects; parameters; field paths; record IDs; precedence/associativity; unary/binary ambiguity; excluded expressions |
| `P1-STMT-*` | Positive and negative cases for every exact statement shape, optional clauses, clause ordering, separators, and multi-statement input |
| `P1-LIMIT-*` | Input, token, nesting, collection, statement, and identifier limits below/at/above defaults |
| `P1-DIAG-*` | Stable category, code, message, label, byte span, and fixed-width colorless `miette` rendering |
| `P1-BRIDGE-*` | Tracy Phase 0 statements still execute; every broader parsed AST receives frontend `UnsupportedSyntax` |
| `P1-COMPAT-*` | Every matrix row references parser tests and allowed-source provenance; no parser-only feature is mislabeled executable |
| `P1-FUZZ-*` | Arbitrary bytes never panic; successful parses have valid nested spans and safe `Debug`; seed corpus covers all statements and malformed boundaries |

Add a detached cargo-fuzz package under `fastdb-parser/fuzz/`, following the repository fuzz layout, with an independently authored `parse` target and seed corpus. Install nightly Rust and `cargo-fuzz` locally if absent, record their versions, and run:

```sh
cargo +nightly fuzz run parse -- -max_total_time=300
```

Required local verification:

```sh
cargo metadata --no-deps --format-version 1
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p turso_fastdb_tests
cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run
git diff --check
```

## 4. Definition of Done

Phase 1 is complete only when:

- Repository Actions remain disabled and the setting is verified.
- All FastDB-authored examples use Tracy.
- Every MVP grammar form has positive and negative parser tests.
- Every accepted AST node is spanned and engine-independent.
- Limits prevent unbounded recursion and allocation.
- Unsupported syntax is never silently accepted.
- The cargo-fuzz corpus and five-minute local run have no crash or hang.
- `COMPAT.md`, research notes, tests, and report agree.
- The Phase 0 executable slice and all 66 baseline tests remain green.
- No inherited Turso implementation or workflow file changed.

## 5. Assumptions and References

- GitHub Actions are disabled through repository settings, not workflow churn, following GitHub's repository Actions settings documentation.
- Compatibility research starts with official SurrealDB documentation for comments, operators, record IDs, CRUD statements, schema definitions, and transactions, and resolves ambiguity with the official SurrealDB `v3.1.5` release.
- The parser contract is internal until Phase 4; Phase 1 changes do not stabilize a public API or database format.
- New parser dependencies are avoided unless necessary. Any added dependency follows repository license and NOTICE policy.
- No Phase 1 result may claim complete compatibility, production readiness, cloud readiness, or release-quality performance.
