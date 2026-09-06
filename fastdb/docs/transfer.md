# Document transfer format v1

The Rust `Connection::export_documents(table, TransferFormat::{Json, Ndjson})` API returns a string. `import_documents(table, input, format)` inserts every document atomically and returns the count. Both target an existing collection. This format transfers data only: it does not include validation definitions, indexes, relational tables, migrations or database recovery state.

CLI usage:

```sh
fastdb-cli --export docs source.db > docs.json
fastdb-cli --import docs destination.db < docs.json
fastdb-cli --export docs --ndjson source.db > docs.ndjson
fastdb-cli --import docs --ndjson destination.db < docs.ndjson
```

Create the target collection and its desired validators/indexes before importing. Existing IDs and unique conflicts fail the whole import; there is no implicit overwrite or upsert. Missing IDs use the usual automatic-ID insertion behavior. Present IDs must identify the target collection; imports do not retarget references. A failed import leaves no partial imported data or index entries. Engine errors can abort an enclosing Rust transaction according to the existing transaction contract. The CLI exits nonzero and writes transfer errors to stderr; successful imports print `{"imported":N}`. Export stdout contains only the transfer payload.

JSON structure:

```json
{"header":{"format":"fastdb.documents","version":1},"documents":[{"type":"Object","value":{"id":{"type":"Record","value":{"table":"docs","key":{"type":"Integer","value":"9223372036854775807"}}},"count":{"type":"Integer","value":"9223372036854775807"}}}]}
```

NDJSON begins with the header object alone on its first line. Each following line is one typed Object entry, with no outer documents array. Exports include a final newline; imports also accept an unterminated final line and CRLF. Blank lines are rejected. Empty transfers use an empty JSON documents array or a header-only NDJSON file.

Values are tagged objects with case-sensitive `type` and, except Null, `value` fields:

| Tag | Value payload |
| --- | --- |
| Null | No payload |
| Boolean | JSON boolean |
| Integer | Canonical signed decimal int64 string, no plus sign or leading zeros |
| Number | String of 16 lowercase hex digits encoding finite IEEE-754 binary64 bits, including signed zero |
| String | JSON string, never interpreted as a reference |
| Binary | Array of byte integers 0–255 |
| Vector | Array of bytes in a validated pinned-engine vector encoding |
| Record | `table` string and `key` tagged Integer or String, with integer keys encoded as decimal strings |
| Object | Map of field names to tagged values |
| Array | Array of tagged values |

The wrapper prevents user objects with `type`/`value` fields from colliding with tags. Unknown header versions, unknown typed fields/tags and invalid values are rejected. Transfer JSON is distinct from the prototype CLI query-result JSON: clients must not assume those result envelopes already use this portable encoding. JavaScript consumers can decode Integer payloads with BigInt without passing them through Number. No Node client implementation is implied.

Current limits are 64 MiB of encoded input/output and 100,000 documents per operation. Values retain normal validation and nesting limits; JSON parsing also has its own recursion bound. Transfers currently materialize documents and JSON in memory, so these bounds are not peak-memory guarantees. Export reads share a statement snapshot and order by stored ID; that byte order is not a portable semantic sort order. Input parsing completes before mutations. Duplicate document fields are rejected, including nested objects. Streaming, relational data transfer, total memory accounting, large-transfer benchmarks and full restore rehearsals remain release work.
