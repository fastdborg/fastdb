# FastDB 2.1.0 for Linux x64

This bundle contains the embedded FastDB engine, FastQL V2 and native clients.
Use one owning process per database file; see DEPLOYMENT.md and OPERATIONS.md.
The supported target is a Linux application server embedding FastDB; no graph
database API or separate network database daemon is promised.
Verify its checksum file before use. The release record and manifest describe
the exact source, build profile and tested platform. Back up V1 databases before
opening them with V2. A database using V2 catalog features cannot be downgraded
to V1; follow BACKUP.md for restore procedures.

The target baseline is Ubuntu 24.04 x86_64 under WSL2 (glibc 2.39). Install the
distribution's libc6, libgcc-s1 and libstdc++6 packages. The candidate's exact ELF
symbol requirements and installed-runtime results must be recorded in its
qualification evidence before publication. Symbol floors are not a tested
distribution matrix. Alpine/musl and other architectures are outside this bundle.

## Install a client

Node.js 22 or newer:

```sh
pnpm add /absolute/path/to/bundle/packages/fastdb-node-2.1.0.tgz
```

Python 3.10 or newer:

```sh
python -m pip install /absolute/path/to/bundle/packages/fastdb_embedded-2.1.0-cp310-abi3-*.whl
```

The wheel tag expresses its build compatibility check. The release's actual
tested host is recorded separately; it does not imply tests on every Linux
distribution. Node and Python contain their own native engine libraries.

PHP, Swift, C# and Go use the shared native library under `lib/`. Add that
directory to the loader search path before starting the application, for example:

```sh
export LD_LIBRARY_PATH=/absolute/path/to/bundle/lib
```

Extract the corresponding source package from `packages/` and follow its README.
PHP requires FFI and an explicit library path. Swift uses the included SwiftPM
package plus linker `-L`/runtime `rpath`. Go uses a local module replacement,
cgo and `CGO_LDFLAGS=-L/absolute/path/to/bundle/lib`. C# consumes the local NuGet
package and requires the same native library at runtime. Package-registry
publication is not implied by this bundle. Rust uses the included pinned source
archive and a path dependency on `fastdb/frontend` with Rust 1.88.0.

## Query documents and spatial data

```javascript
const { Database } = require('@fastdb/node');
const db = new Database('places.db');
try {
  db.execute("INSERT INTO places {id:places:cafe,title:'Coffee',p:geo::point(100,13)}");
  db.execute('CREATE SEARCH INDEX nearby ON places(p) USING SPATIAL');
  console.log(db.all("SELECT id FROM search::near('nearby',geo::point(100,13),1000)"));
} finally {
  db.close();
}
```

Use a new database for this example. The Node `AsyncDatabase` API runs database
work on a worker thread and is appropriate for request handlers. Browser UIs
communicate with an application backend. No browser runtime or cloud hosting is
included here.

`bin/fastdb-cli` accepts SQL and FastQL scripts. V2 adds indexed spatial/H3,
FTS and ANN search, record brace projections, indexed inverse relationships and
sandboxed JavaScript functions. Read the feature contracts in the source archive
for supported forms and limits. Preserve positional result columns and inspect
transaction reports after an error. Results may materialize in memory;
cancellation is cooperative and configured limits are not a total memory cap.

Native component attribution is under `notices/`, with package-specific copies
where the package embeds a native engine. The source archive includes provenance,
the core exception maintenance register, release evidence and detailed contracts.

## Adopt SQLite files

```sh
bin/fastdb-cli sqlite check application.sqlite
bin/fastdb-cli sqlite import application.sqlite application.fastdb
```

The source is preserved and committed WAL data is included. Unsupported schemas
are rejected before a destination is published. Tables remain relational. Read
SQLITE-ADOPTION.md for the supported-schema boundary and migration procedure.
