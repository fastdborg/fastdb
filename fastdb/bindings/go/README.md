# FastDB for Go

Native embedded FastDB/FastQL through cgo. This is a local development module;
see [qualification and limits](../../docs/native-language-clients.md).

Build the native library from the repository root:

```sh
cargo build --locked -p fastdb-c
```

Use a local `replace` for module
`github.com/fastdborg/fastdb/fastdb/bindings/go/v2` pointing at this directory.
Set `CGO_LDFLAGS=-L/absolute/path/to/target/debug` for linking and
`LD_LIBRARY_PATH=/absolute/path/to/target/debug` on Linux for execution.

```go
db, err := fastdb.Open("app.db")
if err != nil { return err }
defer db.Close()
result, err := db.Execute("SELECT $n", map[string]fastdb.Value{
    "$n": fastdb.Integer(9223372036854775807),
}, -1)
if err != nil { return err }
n, err := result.Rows[0][0].Int64()
```

`Call` exposes the shared protocol. `*fastdb.Error` includes `Code`, `Message`
and transaction state. Timeouts are milliseconds, with `-1` for none.
Call `Close` explicitly, and synchronize an entire multi-call transaction.
Run `go test -race -v ./...` with the linker/loader paths configured.
