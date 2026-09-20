# Cloud CLI

The generic cloud client is implemented locally, but has not been included in a
published FastDB artifact. The hosted API must support authenticated `/v1` routes.

Set `FASTDB_API_KEY` in the process environment. The CLI never accepts it as a
command-line argument. The default endpoint is `https://cloud.fastdb.org`;
`FASTDB_CLOUD_URL` can select another HTTPS origin. Plain HTTP is accepted only
for loopback development. Redirects are disabled and TLS certificate verification
remains enabled.

```sh
fastdb cloud whoami
fastdb cloud db create example
fastdb cloud db list
fastdb cloud db show DATABASE_UUID
fastdb cloud db access DATABASE_UUID
fastdb cloud db delete DATABASE_UUID
```

Commands emit JSON to stdout. `access` reads SQL/FastQL from the terminal or stdin,
uses the existing multiline parser, and submits each completed batch atomically.
Each batch has at most 32 statements and a 64 KiB serialized request. Query access
requires `query` and `databases:read` scopes. The server enforces ownership and
entitlements. URLs returned in database metadata never redirect the client's
credentials to a different host.

Inside `access`, `.quit` exits, `.clear` clears unsubmitted input, `.help` shows
usage and `.retry` retries an unresolved request with its original UUID, expected
sequence and statements. Transport/server failures may have committed: the CLI
blocks new SQL until the receipt resolves or the session exits. After an uncertain
attempt, a later rejection does not establish that the original write failed.
Request identity and sequence appear in error output, and a session that observed
an error exits nonzero even if a later retry succeeds.

Pending requests are retained in memory only. Before exiting an unresolved session,
keep the original SQL and the printed request ID/sequence for investigation. Do not
resubmit an uncertain write with a new request ID merely because the process was
restarted. The CLI does not persist SQL or credentials to a history/recovery file.

Responses are bounded to 512 KiB and requests time out after 120 seconds. API keys
are redacted from errors and JSON output. These client bounds are separate from
server query execution, storage and account quotas.

Generic client checks: `python3 fastdb/scripts/check-cloud-cli.py target/debug/fastdb-cli`.
Cloudflare-backed checks live in the separate private service repository.

## Optional read-only request

The next CLI candidate adds a single-request command:

```sh
printf 'SELECT n FROM items WHERE id=1;\n' | fastdb cloud db read DATABASE_UUID
```

It reads SQL/FastQL from stdin and sends one authenticated POST to the database's
`/read` endpoint. It needs the `query` scope and ownership (or a matching scoped
key), without `databases:read` or a preliminary database-info request. Output is
the typed JSON response. The server enforces the supported SELECT-only subset;
write statements are rejected. Up to32statements and64KiBencoded request body
are accepted, subject to server limits.

This command creates no replay receipt and does not advance database sequence.
It never retries automatically. Running it again may observe newer committed
data. Use `db access` for writes and its existing receipt-backed retry behavior.
The previously packaged September15candidate does not include this command;
updated artifact qualification and publication remain pending.
