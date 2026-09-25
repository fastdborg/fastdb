#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --locked -p fastdb-c
case "$(uname -s)" in
  Linux) library=libfastdb_c.so ;;
  Darwin) library=libfastdb_c.dylib ;;
  *) echo 'C ABI checks currently support Linux and macOS hosts' >&2; exit 1 ;;
esac
python3 fastdb/scripts/check-c-abi.py "${CARGO_TARGET_DIR:-target}/debug/$library"
