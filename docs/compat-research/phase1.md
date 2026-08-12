# Phase 1 clean-room grammar characterization

Status: completed 2026-08-12. The record-ID ambiguity was resolved by design review in favor of the SurrealDB `v3.1.5` backtick form.

## Method and provenance

This note uses only public SurrealQL documentation and independently designed black-box queries against an unmodified official binary. No SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus was read, copied, translated, or committed.

Public sources accessed 2026-08-12:

- [Comments](https://surrealdb.com/docs/reference/query-language/language-primitives/comments)
- [Operators](https://surrealdb.com/docs/reference/query-language/language-primitives/operators)
- [Record IDs](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/record-ids)
- [Statement overview](https://surrealdb.com/docs/reference/query-language/statements/overview), including the linked CREATE, SELECT, UPDATE, and DELETE pages
- [DEFINE TABLE](https://surrealdb.com/docs/reference/query-language/statements/define/table), [DEFINE FIELD](https://surrealdb.com/docs/reference/query-language/statements/define/field), and [DEFINE INDEX](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
- [Transactions](https://surrealdb.com/docs/reference/query-language/language-primitives/transactions)
- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)

Reference environment:

- Asset: `https://github.com/surrealdb/surrealdb/releases/download/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`
- Archive SHA-256: `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`
- `surreal version`: `3.1.5 for linux on x86_64`
- Host: Linux x86_64, kernel `7.0.0-29-generic`
- Backend: `mem://`; namespace `fastdb`; database `phase1`
- CLI: `surreal sql --hide-welcome --json --log none`, adding `--multi` for physical-newline string probes
- The archive and binary remained under `/tmp` and are not committed.

JSON below is exact apart from prompts and terminal formatting. Error text retains the material parser message and source span but omits repeated terminal rendering.

## Lexical observations

### Comments

| Input | Result |
| --- | --- |
| `RETURN 'hash'; # comment` | `["hash"]` |
| `RETURN 'slash'; // comment` | `["slash"]` |
| `RETURN 'dash'; -- comment` | `["dash"]` |
| `RETURN 'block'; /* comment */ RETURN 'after-block';` | `["block","after-block"]` |
| `RETURN 'before'; /* unterminated` | `Unexpected end of file, expected multi-line comment to end`, label on `/*` |
| `RETURN 'before'; /* outer /* inner */ tail */ RETURN 'after';` | The first `*/` closes the comment; trailing `*/` is then rejected (`Failed to lex regex, unexpected eof`) |

Block comments therefore do not nest. Comment introducers inside a string are data.

### Strings and escapes

| Input | Result |
| --- | --- |
| `RETURN ['single', "double", 'O\'Brien', "a\"b", 'line\nnext', "unicode café 😺"];` | `[["single","double","O'Brien","a\"b","line\nnext","unicode café 😺"]]` |
| physical newlines inside both quote styles, submitted with `--multi` | `[ ["first line\nsecond line","double line\nnext"] ]` |
| `RETURN ["slash\\", "tab\t", "return\r", "back\b", "form\f", "\u0061"];` | `[["slash\\","tab\t","return\r","back\b","form\f","a"]]` |
| `RETURN 'bad\q';` | `Invalid escape sequence`, label on `\q` |
| `RETURN "\/";` | `Invalid escape sequence`, label on `\/` |
| `RETURN ['O''Brien', "a""b"];` | second adjacent string token rejected; doubled-delimiter escaping is not reference syntax |

FastDB accepts the observed backslash escapes and physical multiline content. It also retains doubled single-quote escaping only for the pre-existing executable Phase 0 seam; `COMPAT.md` records that narrow legacy extension.

### Identifiers and keyword case

`rEtUrN [TrUe, FaLsE, NuLl];` produced `[[true,false,null]]`, confirming case-insensitive keywords for the probed tokens. `RETURN Person:MiXeD; RETURN PERSON:MiXeD;` produced `["Person:MiXeD","PERSON:MiXeD"]`, confirming that identifier spelling and case are preserved. FastDB follows these observations.

### Numbers

| Input | Result |
| --- | --- |
| `RETURN [0, -1, +1, 1.5, 1e3, 1E-3];` | `[[0,-1,1,1.5,1000.0,0.001]]` |
| `RETURN .5;` | `Unexpected token '.', expected an expression` |
| `RETURN 01;` | `[1]` |
| `RETURN 1.;` | error at `;`, expected an identifier after `.` |
| `RETURN 1e+;` | `Invalid number token, expected a digit` |
| `RETURN 9223372036854775807;` / `RETURN -9223372036854775808;` | accepted at the signed-64-bit boundaries |
| either signed-64-bit boundary exceeded | `number cannot fit within a 64bit signed integer` |
| `RETURN 1e309;` | `[null]` in strict JSON output |

FastDB validates integer literals as `i64` and accepts only finite `f64` values, so an overflowed exponent is rejected instead of entering an AST that cannot round-trip through the MVP JSONB representation. Bare `NaN` and `inf` are identifiers in the reference grammar, not non-finite numeric literal spellings; FastDB does not reserve or synthesize them.

## Values, record IDs, and expressions

### Record-ID components

| Input | Result |
| --- | --- |
| `RETURN person:bare;` | `["person:bare"]` |
| ``RETURN person:`quoted id`;`` | ``["person:`quoted id`"]`` |
| `RETURN person:-7;` | `["person:-7"]` |
| `RETURN person:'quoted id';` | `Unexpected token 'a strand', expected an identifier` |

The original plan wording “quoted string” was ambiguous. Design review on 2026-08-12 selected the compatibility-preserving interpretation: complex text uses backticks; single and double quotes remain value-string delimiters. `plan-phase1.md` and `revised_plan.md` now say backtick-quoted text.

### Collections

`RETURN [[1,2,], {a: 1, a: 2, 'q': 3,}];` produced `[[[1,2],{"a":2,"q":3}]]`. Arrays and objects accept one trailing comma. The reference resolves a duplicate object key to its last value; the FastDB parser deliberately preserves both fields in source order because Phase 1 is syntax/AST only and later schema/execution phases must choose the semantic policy explicitly. Double commas in either collection were rejected at the second comma.

### Binding power and exclusions

`RETURN [1 + 2 * 3, 8 / 4 / 2, 10 - 3 - 2];` produced `[[7,1,5]]`, confirming multiplicative-before-additive binding and left associativity for the probed arithmetic operators. `RETURN true AND false OR true;` produced `[true]`; `RETURN 1 < 2 = true;` produced `[true]`.

The reference accepts symbolic `!`, `&&`, `||`, `==`, `%`, and `**` in the probes. The fixed FastDB MVP grammar intentionally narrows this surface to keyword `NOT`, `AND`, and `OR`, single `=`, and the documented arithmetic set; it rejects the symbolic aliases and modulo/power at their introducing token. The reference probe `RETURN NOT false;` was rejected, so keyword `NOT` is a deliberate FastDB grammar spelling rather than a claim of exact v3.1.5 syntax. This divergence is explicit in `COMPAT.md` and remains parser-only in Phase 1.

## Statements and clauses

This independently authored reference batch parsed and executed without error:

```surql
DEFINE TABLE person SCHEMAFULL;
DEFINE FIELD name ON TABLE person TYPE string;
DEFINE INDEX by_name ON TABLE person FIELDS name UNIQUE;
CREATE ONLY person:tracy CONTENT {name: 'Tracy'} RETURN AFTER;
SELECT name AS label FROM ONLY person WHERE name = 'Tracy'
  ORDER BY name DESC LIMIT 1 START 0;
UPDATE person:tracy SET name = 'Tracy' WHERE name = 'Tracy' RETURN AFTER;
DELETE person:tracy WHERE name = 'Tracy' RETURN BEFORE;
BEGIN TRANSACTION; COMMIT TRANSACTION; BEGIN; CANCEL;
```

Strict JSON output was:

```json
[null,null,null,{"id":"person:tracy","name":"Tracy"},{"label":"Tracy"},[{"id":"person:tracy","name":"Tracy"}],[{"id":"person:tracy","name":"Tracy"}],null,null,null,null]
```

The public statement pages describe broader clauses including `FETCH`, `GROUP`, `SPLIT`, `OMIT`, `EXPLAIN`, `TIMEOUT`, `PARALLEL`, update `MERGE`, and additional index kinds. They are excluded from the exact FastDB MVP grammar. The lexer recognizes their introducers so the parser returns a spanned `fastdb::parse::unsupported_syntax` diagnostic rather than ignoring them. The reference accepts optional `TRANSACTION` after `BEGIN`, `COMMIT`, and `CANCEL`; FastDB Phase 1 accepts only the exact bare transaction spellings in the fixed contract and diagnoses the suffix as unsupported.

## Phase 1 interpretation

The reference observations inform delimiters, escapes, boundaries, binding behavior, and precise exclusions; they do not expand the MVP. Where FastDB deliberately differs—legacy doubled single quotes, keyword `NOT`, finite-float rejection, duplicate-key preservation in the syntax AST, and bare-only transaction spellings—the difference is explicit and parser acceptance is still `Planned`, not executable compatibility.
