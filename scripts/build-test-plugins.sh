#!/bin/bash
# Builds the dev-only VST3 fixture plugins (test-plugins/synth, test-plugins/distortion) into
# test-plugins/<name>/target/bundled/<Name>.vst3. Only the VST3 host tests use them (crates/daw-vst3/tests,
# crates/daw-bootstrap/tests, crates/daw-rpc/tests); the app neither ships nor loads them. Without them those tests
# print "SKIPPED" and pass.
set -euo pipefail
# rustup from Homebrew doesn't put cargo on PATH by itself; add it only if it is installed there.
if [ -d /opt/homebrew/opt/rustup/bin ]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
for name in synth distortion; do
  cd "$root/test-plugins/$name"
  # Each fixture is a separate workspace that the root `cargo test --workspace` doesn't check: run tests and clippy here.
  cargo test --workspace
  cargo clippy --workspace --all-targets -- -D warnings
  cargo xtask bundle "$name" --release
done
echo "Done: $root/test-plugins/{synth,distortion}/target/bundled"
