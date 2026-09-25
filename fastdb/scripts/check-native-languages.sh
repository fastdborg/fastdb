#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
root="$PWD"
cargo fmt -p fastdb-c -- --check
cargo clippy --locked -p fastdb-c --all-targets --no-deps -- -D warnings
cargo build --locked -p fastdb-c
export LD_LIBRARY_PATH="${CARGO_TARGET_DIR:-$root/target}/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export FASTDB_LIBRARY="${CARGO_TARGET_DIR:-$root/target}/debug/libfastdb_c.so"
export FASTDB_FIXTURE="$root/fastdb/bindings/fixtures/native-client.json"
export CGO_ENABLED=1
export CGO_LDFLAGS="-L${CARGO_TARGET_DIR:-$root/target}/debug"
cmp fastdb/bindings/c/include/fastdb.h fastdb/bindings/go/include/fastdb.h
cmp fastdb/bindings/c/include/fastdb.h fastdb/bindings/swift/Sources/CFastDB/fastdb.h
python3 fastdb/scripts/check-c-abi.py "$FASTDB_LIBRARY"
php -d ffi.enable=1 fastdb/bindings/php/tests/smoke.php
(cd fastdb/bindings/go && go test -race -v ./...)
DOTNET_CLI_TELEMETRY_OPTOUT=1 dotnet run --project fastdb/bindings/csharp/Tests -- "$FASTDB_FIXTURE"
(cd fastdb/bindings/swift && swift test -j 2 -Xlinker "-L${CARGO_TARGET_DIR:-$root/target}/debug" -Xlinker -rpath -Xlinker "${CARGO_TARGET_DIR:-$root/target}/debug")
