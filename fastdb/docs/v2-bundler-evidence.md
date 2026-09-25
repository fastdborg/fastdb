# V2 browser bundler qualification

Working-tree qualification, 2026-09-25. The package remains private and unpublished.
This qualifies an explicit Vite asset setup, not arbitrary bundlers or SSR.

## Reproduced failure and supported setup

A fresh offline consumer of the preceding browser tarball builds with Vite 8.3.1,
but opening a database with default worker URLs fails in Chromium. Vite moves the
coordinator into its hashed assets directory, while the coordinator's classic
`importScripts('./wasm-util.js', './wasi-threads.js')` still expects adjacent
unchanged helper names. The control reports a NetworkError loading
`/nested/app/assets/wasm-util.js`. Log: `/tmp/fastdb-v2-vite-control.log`.

The package now provides `fastdb-copy-assets`. It copies the installed `dist/`
files, all collected notices and the package license into an explicitly selected
public directory. The application imports the public package normally, then
passes absolute worker and WASM URLs from that copied directory. The documented
Vite example uses `import.meta.env.BASE_URL`, which preserves nested deployment
paths. The runtime API and WASM binary are unchanged. Default automatic bundling
of the worker graph is not claimed to work.

The setup follows Vite's [public-directory contract](https://vite.dev/guide/assets#the-public-directory):
files retain their names during development and copying to the build output.
Both development and preview servers need the cross-origin isolation headers;
production hosting must supply them as well. Other bundlers and SSR remain
outside this qualification.

## Installed-package evidence

`fastdb/bindings/browser/tests/vite.mjs` creates a fresh temporary application,
installs the packed tarball offline with pnpm, invokes the actual installed
`pnpm exec fastdb-copy-assets public/fastdb` command, and checks every copied
runtime file plus LICENSE.md byte for byte against the installed package.
It uses a bare `@fastdb/browser` import in the app, not a checkout source import.
The harness closes browsers/servers and removes the temporary application.

All four combinations pass with zero failed requests:

| Vite mode | Browser | Result |
|---|---|---|
| Development server | Chromium 149.0.7827.55 | Pass |
| Development server | Firefox 151.0 | Pass |
| Production build and preview | Chromium 149.0.7827.55 | Pass |
| Production build and preview | Firefox 151.0 | Pass |

Each runs under `/nested/app/`, creates a named OPFS database, writes a typed record
and maximum int64, creates a spatial index, rolls back an update, verifies H3 and
indexed search, and requires native integrity `ok`. It closes the database,
reloads the page and reopens the same name to verify all values and the index.
This does not replace the separate storage-fault or upgrade suites.

Node 24.19.0, pnpm 11.23.0, Playwright 1.61.0, Linux x86_64. Vite 8.3.1 is pinned
as a development-only dependency. Its explicit version request added a scoped
minimum-release-age exception in the local pnpm workspace settings; runtime
package dependencies are unchanged.

From `fastdb/bindings/browser`:

```sh
node tests/vite.mjs /tmp/fastdb-v2-bundler-packages/fastdb-browser-2.0.0-dev.1.tgz
```

Final log: `/tmp/fastdb-v2-vite-check-final.log`.
Build/pack log: `/tmp/fastdb-v2-vite-pack.log`.
Artifact: `/tmp/fastdb-v2-bundler-packages/fastdb-browser-2.0.0-dev.1.tgz`,
**6,026,028 bytes**, SHA-256
`c65be89af2ff30949b2ac71a0b2fcbb34ce48ff1d312ba3cdac7ceb44cecefcb`.
All packaged `dist/` files match the preceding notice-qualified tarball byte for
byte. WASM SHA-256 remains
`637cb8b541daa1c0a284a25694d5ef4bbf2f2cd8c5fc3f1aaca7a3f5b3a7e014`.
Its browser FTS exclusion and the other open platform/attribution/release gates
remain unchanged. See the [client README](../bindings/browser/README.md) for the
copy command and application code.
