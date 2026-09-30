# Cloud CLI (0.4 protocol)

## Candidate package

From a clean committed engine checkout, with Rust 1.88.0, Node 24, Python 3.12+
and the locked Cargo cache available:

```sh
/home/tan/Sites/fastdb/scripts/fastdb-heavy python3 fastdb/scripts/build-cloud-cli.py /home/tan/fastdb-cloud-cli-0.4.0-read-v2
```

The output directory must be new. The builder verifies the CLI dependency/notice
records, uses the checked `fastdb-production` profile, tests the stripped binary
against both synthetic Cloud protocol harnesses, and packages source, notices,
checksums and build evidence. The embedded CLI version remains 2.1.0; this separate
candidate targets Cloud protocol 0.4.0 and does not replace published V2 artifacts.
The archive is `fastdb-cloud-cli-0.4.0-read-v2-linux-x64.tar.gz`; its manifest
records read protocol version 2, read journal version 3 and query journal version 2.
The original `fastdb-cloud-cli-0.4.0-linux-x64.tar.gz` uses the historical read
format and is incompatible with current beta reads. Keep old uncertain journals
for reconciliation rather than translating them.
`SOURCE_DATE_EPOCH` defaults to the source commit timestamp; an explicit nonnegative
value is honored and recorded for reproducible builds. The candidate still needs
acceptance against the native Cloud service and release qualification before
publication. It does not fall back to the old 0.2 database routes.

## Connecting

Set `FASTDB_API_KEY` to an organization key (`fdbo_…`). Credentials are accepted
only through the environment. `FASTDB_CLOUD_URL` defaults to
`https://cloud.fastdb.org`; another endpoint must be an HTTPS origin, with HTTP
allowed only on loopback for local development. Redirects and implicit HTTP
retries are disabled. TLS verification remains enabled.

Select an organization with `FASTDB_ORGANIZATION_ID` or the leading
`--organization UUID` flag. The flag overrides the environment. Database commands
require an explicit selection and use organization URLs; there is no fallback to
the 0.2 URL layout. The server checks that the current key permits that organization
and database on every operation. Discovery needs no selected organization.

```sh
fastdb cloud whoami
fastdb cloud organizations
fastdb cloud --organization ORGANIZATION_UUID db list
fastdb cloud --organization ORGANIZATION_UUID db create example
fastdb cloud --organization ORGANIZATION_UUID db show DATABASE_UUID
fastdb cloud --organization ORGANIZATION_UUID db delete DATABASE_UUID
```

Management requires `manage`; listing/show requires `read`. Creation can remain
pending while capacity is prepared. After an uncertain creation reply, inspect the
organization's database list and original name before issuing further operations.
Responses are bounded to 512 KiB, individual requests time out after 120 seconds,
and API keys are redacted from returned errors and JSON output.

## Journaled reads and writes

With `FASTDB_ORGANIZATION_ID` set:

```sh
fastdb cloud db read DATABASE_UUID read.json < query.sql
fastdb cloud db query DATABASE_UUID write.json < mutation.sql
fastdb cloud db retry read.json
fastdb cloud db retry write.json
```

`read` needs `read` scope; `query` needs `read` and `query`. Both parse up to 32
SQL/FastQL statements in at most 64 KiB of encoded request data. The server's native
read endpoint enforces read-only execution; the CLI does not infer safety from a
SQL prefix. Read requests send `readVersion: 2`. Every read attempt executes a
fresh snapshot, including a retry with the same correlation ID. `afterSequence`
requires visibility of acknowledged writes; the returned sequence is a confirmed
causal lower bound and may equal it. Reads do not allocate commit positions.
Queries retain durable request IDs and replay; committed query responses advance
their lower bound by at most 64 positions.

Before submission, the CLI reads the database sequence, creates a fixed request ID,
and atomically saves a new journal containing the endpoint, organization, database,
operation, exact SQL statements and `afterSequence`. Query journals use version 2;
read journals use version 3. Old read journals and version-1 query journals are
rejected before dispatch. It syncs the file and parent
directory before sending the data request. Existing journals are never overwritten.
The journal defaults to owner-only permissions; it **contains SQL**, which can
include sensitive values, but no API credential is added. Keep it out of source
control. Choose a new journal only for an intentionally new operation.

After any uncertain reply or process interruption, `db retry JOURNAL` replays the
saved request without refreshing its lower bound or reading new SQL. Read retries
execute again and may return newer rows. Each attempt has its own quota hold; loss
of metrics can leave the maximum reservation unknown. Writes replay their retained
result without executing again. Response IDs and the versioned causal/replay
position must validate before the command reports success.
Mismatched endpoints/organizations, malformed journals and unconfirmed responses
fail without silently creating a replacement request. A later rejection is not
proof that an earlier dispatch rolled back. The journal remains on disk after
success or failure so the caller can retain or remove it deliberately.

Server receipts have finite retention. The service checks retained receipts before
window expiry. Once a receipt is unavailable, an expired retry window prevents
reexecution but does not establish whether the original request committed.
Resolve the stored outcome before submitting an uncertain write under a new ID.
This journal supplies durable client identity; it does not extend server retention
or establish complete billing enforcement. Reconcile pending version-1 journals
with the older service/client before upgrading; never translate and resubmit an
uncertain operation. See [the retry contract](cloud-query-protocol-v2.md).

## Interactive access

```sh
fastdb cloud --organization ORGANIZATION_UUID db access DATABASE_UUID
```

Access requires `read` and `query`. SQL/FastQL ends in semicolons; each submitted
batch commits atomically. `.quit` exits, `.clear` clears unsubmitted input,
`.help` shows usage and `.retry` reuses the pending request ID, sequence and SQL.
Uncertain failures or unconfirmed response identities block new SQL until resolved.
A session that observed an error exits nonzero even if its retry later succeeds.

Interactive pending state is in memory only. Use journaled `db query` for writes
that must survive CLI process loss. An unresolved interactive exit reports its
request ID and sequence; do not treat it as proof of rollback or blindly resubmit.
No SQL or credentials are written to interactive history. Commands emit JSON to
stdout; one-shot request recovery instructions go to stderr.

This source targets the breaking 0.4 protocol. It is not a published replacement
CLI artifact yet. Clean-source packaging, final artifact qualification and hosted
release gates remain open. Generic HTTP checks run with
`python3 fastdb/scripts/check-cloud-cli.py target/debug/fastdb-cli`; private service
acceptance lives in the Cloud repository.

## Resumable SQLite imports

The client supports organization-scoped imports using the same service as the
dashboard. Customer starts must be enabled by the service. Use an unrestricted
organization key with `manage` scope and current verified Owner/Admin authority.
The server rechecks access, capacity and quotas throughout the job.

```sh
fastdb cloud import start ORGANIZATION_UUID example snapshot.sqlite import.json
fastdb cloud import resume import.json snapshot.sqlite
fastdb cloud import status ORGANIZATION_UUID IMPORT_UUID
fastdb cloud import cancel ORGANIZATION_UUID IMPORT_UUID --confirm
```

Make a consistent SQLite backup/export first. Copying an open main database file
may omit committed WAL changes. The client accepts a regular file from 4 KiB to
10 GB, aligned to 4 KiB, with a SQLite header; the service performs authoritative
integrity, schema and native compatibility checks before publication. These limits
do not promise successful execution of a 10 GB source on every serving plan.

`start` hashes bounded 8 MiB chunks and atomically creates the explicit journal
without overwriting an existing file. It syncs both the file and parent directory
before dispatching any request. The immutable journal contains the endpoint,
organization, import/destination UUIDs, name, size and SHA-256, never the API key or
source contents. Its default permissions allow only the current user to read it.
Keep this file after interruptions and use `resume`, rather than issuing another
`start`. Use a separate journal for each intentionally new import. The client does
not delete journals when a job finishes.

`resume` binds the journal to the configured endpoint, verifies the exact source,
follows bounded receipt pages and skips only matching confirmed parts. Each chunk
is checked again before sending so changes during upload fail visibly. If creation
was never acknowledged and the server reports the job missing, the original synced
declaration is replayed. Redirects and implicit HTTP retries remain disabled. A
failed call never creates a replacement identity or implies remote rollback.

The client polls initial capacity reservation up to 30 times, waiting two seconds
between requests; each request retains the 120-second timeout. If it still
reports `creating`, run `resume` later. Once uploaded, it schedules processing and
prints the current job as JSON; use `status` until `ready` or `canceled`. An exit
code of zero means the command was acknowledged, not that a pending import has
finished. Only `state: "ready"` and its verified result establish publication.
A resume of a job already processing or terminal needs no source file read.

Progress and recovery instructions go to stderr; job JSON goes to stdout. Ctrl-C
stops the local process; an in-flight part may still finish. Resume from the same
journal. Cancellation requires `--confirm` and preserves data if publication won
the race. Status/cancel remain available when new imports are disabled. Retain
source and journal until the durable outcome is known.

Linux x64 is the qualification target. Generic fault tests run with
`python3 fastdb/scripts/check-cloud-import-cli.py target/debug/fastdb-cli`; the
private Cloud suite separately exercises the compiled binary with native import,
restart, queried rows and exactly-once usage. This source change is not a published
CLI artifact; clean-source packaging and release qualification remain required.

## Database service JWTs

The CLI source accepts a database-scoped JWT in `FASTDB_API_KEY` for `db show`,
`db read`, `db query`, `db retry`, and `db access`. Set `FASTDB_ORGANIZATION_ID` and
use the database UUID; the CLI does not infer authority from token claims.
Read-only tokens use `db read`. Interactive access needs both read and query.
A database token survives removal of its creator; explicit revocation, expiry
and database deletion prevent new requests. Already-authorized requests may finish.

An organization credential with manage scope can run:

```
fastdb cloud db tokens DATABASE_UUID
fastdb cloud db token-create DATABASE_UUID ISSUANCE_UUID NAME read|write EXPIRES_AT_MS
fastdb cloud db token-revoke DATABASE_UUID TOKEN_UUID
```

Choose a fresh UUIDv4 for issuance and a future expiry in Unix milliseconds,
no more than 90 days away. Retry uncertain creation with exactly the same values.
Creation returns the secret once as JSON; protect that output. Listing never
returns secrets. Dashboard Database → Settings → Manage database tokens provides
30-day tokens. Previously published CLI archives do not include these additions.
