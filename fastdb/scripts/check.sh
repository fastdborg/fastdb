#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo fmt -p fastql-parser -p fastdb -p fastdb-cli -p fastdb-tests -p fastdb-node -- --check
cargo clippy --locked -p fastql-parser -p fastdb -p fastdb-cli -p fastdb-tests -p fastdb-node --all-targets --no-deps -- -D warnings
cargo test --locked -p fastql-parser -p fastdb -p fastdb-cli -p fastdb-tests
fastdb/scripts/check-node.sh
