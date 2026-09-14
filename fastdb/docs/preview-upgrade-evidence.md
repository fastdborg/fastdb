# Preview.1 to preview.2 upgrade rehearsal

N5 subrequirement: rehearse opening a database from the previous released binary.
Passed on Linux x64, Node 24.19.0, using separately installed retained release
packages. Each phase uses a separate Node process and its own native addon.

- Preview.1 writes a collection with a unique email index and an ordinary SQL table.
- Preview.2 opens it, checks exact indexed-query and relational results, and runs
  the collection integrity audit.
- A mixed document/SQL transaction rolls back. Duplicate indexed values remain
  rejected without extra documents. An UPSERT creates a new collection.
- A fresh preview.2 process verifies original data and the newly committed
  collection, including integrity checks.

Reusable command (absolute paths to installed package directories):

```sh
node fastdb/scripts/check-preview-upgrade.cjs /path/to/old/@fastdb/node /path/to/new/@fastdb/node
```

Package SHA-256 identities:

- Preview.1: `44cf4199137f51f58349c28b666c8729d53326a5eb26c280b23f6a148c185393`
- Preview.2: `b001a00960bd5acc46d9005acbbda9c5fad310910982f94ac2c1d63624f7c402`

Run log: `/tmp/fastdb-preview-upgrade.log`. This closes the concrete previous
preview upgrade rehearsal. It does not establish arbitrary older-version,
downgrade, power-loss, or other-platform compatibility. No storage format changed
between these previews; remaining stable-release evidence stays separately tracked.
