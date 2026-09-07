#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
build_profile=debug
cargo_build_args=()
case "${1:-}" in
  '') ;;
  --release) build_profile=release; cargo_build_args+=(--release) ;;
  *) echo 'Usage: check-node.sh [--release]' >&2; exit 2 ;;
esac
if (( $# > 1 )); then
  echo 'Usage: check-node.sh [--release]' >&2
  exit 2
fi
cargo build --locked -p fastdb-node "${cargo_build_args[@]}"
case "$(uname -s)" in
  Linux) native_library=libfastdb_node.so ;;
  Darwin) native_library=libfastdb_node.dylib ;;
  *) echo 'Native build script currently supports Linux and macOS only' >&2; exit 1 ;;
esac
cp "${CARGO_TARGET_DIR:-target}/$build_profile/$native_library" fastdb/bindings/node/fastdb.node
node --test fastdb/bindings/node/test.cjs fastdb/examples/node-task-tracker/test.cjs
npm run typecheck --prefix fastdb/bindings/node
