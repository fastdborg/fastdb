# FastDB CLI

```text
fastdb [--memory | PATH] [-c SOURCE]
       [--output human|json] [--param NAME=JSON]
fastdb shell [--memory | PATH] [-c SOURCE] [OPTIONS]
fastdb check PATH [--output human|json]
fastdb backup PATH DESTINATION [--output human|json]
fastdb restore BACKUP DESTINATION [--output human|json]
fastdb rebuild-index PATH TABLE INDEX [--output human|json]
```

- `-c` executes one request and exits.
- Piped stdin is one batch request.
- A terminal starts the interactive shell. Multiline input is complete when
  lexer/parser state is complete; a trailing semicolon is neither required nor
  sufficient. Strings and comments may contain semicolons.
- The shell keeps no persistent history. Parameter values are not retained.
- `--param` binds a JSON value. Names omit `$`, are case-sensitive, and may
  appear only once.
- Batch failures exit 1. Argument errors use clap's conventional exit 2.
- The legacy invocation remains an alias for `shell`.

Operational commands take an exclusive maintenance lease. `check` validates
catalogs, hidden provider state, graph adjacency indexes, vector encodings, and
engine integrity. `backup` checkpoints and validates a new destination before
publishing it. `restore` validates both the source and the copied temporary
file before atomically publishing a new destination. Neither command
overwrites an existing path. `rebuild-index` resolves logical table/index names
through the catalog and rebuilds the sealed provider representation.

Full-text definitions and queries work in command, piped-batch, and
interactive modes. Quote the source at the shell boundary and use `--param`
for runtime query text:

```text
fastdb app.fastdb --param q='"Rust database"' -c \
  'SELECT id FROM article WHERE body @@ $q'
```

The Surreal-compatible surface is limited to the documented `blank` analyzer,
single-field FULLTEXT index, one `@@`/`@n@` predicate, and supported
`search::*` functions. Native `USING fts` and `fts_*` syntax is a FastDB/Turso
extension. Neither query text nor parameters are logged by FastDB.

Human output starts every result with `-- statement N --` and prints object
keys in lexical order. JSON mode writes exactly one object for each request.
Success goes to stdout; errors go to stderr.

## Strict JSON

Strict values reserve the collision-safe envelope:

```json
{"$fastdb":{"v":1,"t":"rid","table":"person","id_type":"integer","id":7}}
```

`id_type` is `string`, `integer`, or `uuid`. A user object containing the
reserved `$fastdb` member is recursively escaped:

```json
{"$fastdb":{"v":1,"t":"object","value":{"$fastdb":"user data"}}}
```

Responses use `t: "response"` and contain ordered statement objects. Errors
use `t: "error"`, a stable category, human detail, and a byte span when
available. This versioned JSON contract is distinct from the internal format-1
JSONB codec and does not change the database format.

Operational success uses a compact deterministic envelope with `ok`,
`operation`, and a `report` object. Operational failures use the same strict
`$fastdb` error envelope as query failures. Source and parameters are never
included.
