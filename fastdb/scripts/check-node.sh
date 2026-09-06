#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --locked -p fastdb-node
case "$(uname -s)" in
  Linux) native_library=libfastdb_node.so ;;
  Darwin) native_library=libfastdb_node.dylib ;;
  *) echo 'Native build script currently supports Linux and macOS only' >&2; exit 1 ;;
esac
cp "${CARGO_TARGET_DIR:-target}/debug/$native_library" fastdb/bindings/node/fastdb.node
node --test fastdb/bindings/node/test.cjs
npm run typecheck --prefix fastdb/bindings/node
