# Document accessor performance investigation

The optimized 100,000 × 768 exact top-10 diagnostic takes 47.12 seconds. A smaller comparison uses identical 1,000 seeded 768-dimensional vectors in an ordinary table and a collection. Every warmup and measured result matched its counterpart. Three measured samples follow one warmup, using the existing release CLI in one in-memory process.

| Query | Native median | Collection median |
|---|---:|---:|
| count(*) | 0.88 ms | 0.80 ms |
| count(*) WHERE group_no=7 | 1.12 ms | 91.74 ms |
| min(cosine distance) | 1.89 ms | 261.39 ms |

[Raw observations and identities](benchmark-results/2026-09-07-linux-release-accessor-1000.json). This diagnostic measures minimum distance rather than top-10 sorting. It suggests substantial document-access overhead but does not isolate CPU costs, storage-layout effects or establish general ratios. Native queries run first; timing variance and the tiny fixture limit conclusions.

Code inspection finds two relevant costs. `functions::get` decodes and validates the complete tagged JSON document even for an unrelated scalar field. Typed vector field access then serializes the selected vector into tagged JSON, and `vector_input` immediately decodes it again before returning engine vector bytes. The stored vector bytes themselves are represented as a JSON integer array. Removing validation or silently changing stored encoding is not an acceptable optimization.

The next implementation candidate is a combined document-field/vector-input accessor for directly lowered physical collection fields. It should retain complete document validation and existing field traversal while avoiding the intermediate typed-value serialization/deserialization. Other expressions should retain the existing generic path. Tests must compare the combined and original paths for all five vector encodings, binary vector input, vector text, missing/NULL fields, scalar parents, malformed stored roots and malformed unrelated fields. Outer-join NULLs must retain the original conversion error, rather than silently becoming an engine NULL. Derived fields, parameters, constructors and explicit wrappers require care: only a proven equivalent AST shape should use the combined accessor.

After correctness checks, rebuild the optimized CLI and repeat the diagnostic before claiming improvement. Broader whole-document decoding costs and total query memory remain separate work.


## Combined accessor implementation

Plain `__fastdb_value` expressions passed to vector conversion now use `__fastdb_vector_field`, which retains complete stored-document decoding/validation and converts the selected value directly into engine input. Other AST shapes keep the generic path. The shared conversion preserves errors for NULL/missing values, malformed vectors and invalid stored roots. A differential regression compares both paths across all five encodings, binary/text input, missing/nested fields and malformed unrelated fields.

The full scoped suite passed formatting, Clippy, 311 Rust tests, 35 Node tests and strict TypeScript. One existing trigger-interruption gate remains ignored. The optimized rebuild succeeded. A fresh baseline/new-binary diagnostic with identical results measured collection minimum-cosine medians of 266.19/133.71 ms. Unchanged native and filter workloads also varied; this is initial improvement evidence, not a stable general speedup. See [raw comparison](benchmark-results/2026-09-07-linux-release-vector-field-comparison.json) for hashes and samples. Larger top-10 measurements remain open, as does whole-document decoding overhead.


The completed 100,000 × 768 follow-up also passed reference checks: exact top-10 median fell from 47.12 to 32.71 seconds and primary VM steps fell by 100,000. See [benchmarks.md](benchmarks.md) for raw evidence and limits. Whole-document decoding remains a major candidate for further investigation; the combined accessor does not change storage format or remove that work.


## Vector conversion error transaction disposition

A differential regression now checks raw engine NULL, malformed stored bytes and a stored NULL field through both generic and combined vector-input accessors, from autocommit and an active outer transaction. Both paths return identical error text, leave autocommit, preserve committed rows, discard the pending outer write, and allow a subsequent write. Both vector-field unit tests passed. This pins the observed engine-abort behavior for these conversion failures; it does not promise statement-only rollback for every SELECT error. No production behavior changed. The prior full suite has 311 passing Rust tests; this adds one targeted passing regression.
