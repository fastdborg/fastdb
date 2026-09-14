# S1 language acceptance review

This index follows the fourteen acceptance bullets in the authoritative parent
FastQL.md, section 10. It identifies existing evidence to inspect, not fourteen
new test campaigns or a claim that every SQL combination is supported. The
stable target remains the full embedded scope in v1-release-contract.md.

Paths below are relative to fastdb/. Test files are evidence locations; their
presence alone does not close an acceptance item. The latest recorded scoped
run is 676 Rust tests, one known ignored trigger test, and 103 Node tests; see
recovery-io-evidence.md. Final-candidate acceptance remains separate.

| FastQL acceptance item | Existing evidence location | Review disposition |
|---|---|---|
| 1. Pinned ordinary SQL baseline | UPSTREAM.md; tests/tests/sql_compat.rs | Reviewed: ordinary_sql_matches_the_pinned_engine compares column names and typed rows against direct engine execution, including CRUD, rollback, aliases, CREATE AS SELECT and native ON CONFLICT; malformed_collection_sql_preserves_native_parse_errors_and_active_work compares parse errors and retained transaction state |
| 2. Grammar and parameter ambiguities | parser/src/tests.rs; tests/tests/writes.rs; tests/tests/sql_compat.rs; bindings/node/test.cjs | Reviewed grammar assertions plus focused both-client runtime binding regression; see below |
| 3. Collection versus relational CREATE | tests/tests/catalog.rs; tests/tests/persistence.rs; parser/src/tests.rs | Reviewed: sql_boundaries_and_ordinary_tables verifies native integer primary keys after bare IF NOT EXISTS and CREATE AS SELECT; info_is_logical_and_if_not_exists_never_converts_models asserts the existing document model survives a column-list IF NOT EXISTS; parser tests distinguish the three CREATE routes |
| 4. Record construction and direct targets | tests/tests/references.rs; tests/tests/standalone.rs | Inspect fixed/dynamic/key-type and direct-target assertions |
| 5. Document examples with setup | tests/tests/writes.rs; tests/tests/checks.rs | Compare the master-plan examples with executable fixtures |
| 6. Missing/null and nested mutation | tests/tests/expressions.rs; tests/tests/writes.rs | Inspect path, duplicate-key and immutable-ID assertions |
| 7. Index consistency and recovery | tests/tests/integrity.rs; tests/tests/crash_stress.rs; frontend/src/recovery_io.rs | Recovery evidence belongs to S2/S3; do not create a second recovery campaign here |
| 8. Lossless typed values | tests/tests/numbers.rs; tests/tests/references.rs; tests/tests/vectors.rs; bindings/node/test.cjs | Inspect native primary-key versus document-ID assertions and value round trips |
| 9. One-hop links | tests/tests/links.rs | Reviewed: batching, typed identity, relational targets, transaction snapshots and shared byte budget; select_fetches_are_typed_one_hop_projections asserts missing references return null, fetched references remain unexpanded, and nested fetch/predicate/order/write fetch forms reject |
| 10. SQL-shaped write validation | tests/tests/writes.rs; tests/tests/insert_select.rs; tests/tests/checks.rs | Inspect validation/index failure assertions on supported write routes |
| 11. Field metadata lifecycle | tests/tests/catalog.rs | Explicit metadata-only removal, incompatible definition and reopen regressions exist |
| 12. Row cardinality and helpers | tests/tests/returning.rs; bindings/node/test.cjs; bindings/node/index.d.ts | Both Node clients expose all/first/exactlyOne; collection helpers add their separately documented document cardinality |
| 13. Deferred syntax errors | parser/; docs/contracts.md | Review documented V2/V3 rejection examples and stable error classifications |
| 14. SDK and encoding version review | bindings/node/index.d.ts; docs/node-sdk.md; docs/transfer.md | Embedded SDK/transfer review required; cloud wire protocol follows cloud scope |

## Remaining bounded S1 work

1. Inspect the assertions identified above and record uncovered explicit examples.
   Add a regression only for a missing contractual behavior or reproduced bug.
2. Reconcile the current SELECT/write support with the V1 capability table:
   filters, projections, joins, ordering, pagination and direct record targets.
   Distinguish native rejection, implemented collection support and an actual
   missing required form. Historical status entries are chronological evidence,
   not the current support matrix.
3. Review public value encodings and error classifications, then freeze a concise
   current language contract with links to the completed evidence.

For example, contracts.md restricts USING in collection UPDATE FROM while allowing
supported USING forms in SELECT-derived sources. These are different contexts;
the SELECT oracle in tests/tests/using.rs does not prove direct write-FROM support.
Do not erase the write restriction based on that oracle or infer that every join
context needs expansion merely because SELECT supports more forms.

S1 remains open until these three review actions are complete. This document
does not reopen preview requirements P0–P6.

## Grammar review findings

The parser's sql_is_preserved regression explicitly retains bracket identifiers,
colon-containing strings, contextual aliases, single-quoted aliases and all five
parameter spellings. The native differential test executes quoted/contextual
aliases and colon-containing strings, rather than checking dispatch alone.
standalone_record_constructors_and_parameter_names in tests/tests/writes.rs
compares the pinned rejection of `$name::suffix` with the frontend error. Thus
preserving this token spelling does not imply the pinned engine accepts it.
collection_and_records checks backtick string keys, signed int64 minimum and
overflow rejection. The new Node regression `V1 parameter spellings bind through
native and document routes in both clients` executes all five parameter forms
through native scalar SELECT, collection INSERT RETURNING and collection WHERE.
It checks int64 maximum preservation and independently bound numbered, colon,
at-sign and dollar parameters in one statement. The focused Node 24 run passed
against the existing addon on 2026-09-14; log /tmp/fastdb-v1-parameters.log.
No engine rebuild or full scoped rerun was performed for this test-only change
during the active vector measurement; include it at final scoped acceptance.

Items 1, 2, 3 and 9 now have inspected assertions for their explicit checklist
requirements; no additional test matrix is requested for them. This review does not
claim that all possible SQL statements or CREATE variants have been enumerated.
