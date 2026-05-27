#!/usr/bin/env bash
set -euo pipefail

llvm_major="${LLVM_MAJOR:-22}"
cargo_config="${CARGO_HOME:-/opt/cargo}/config.toml"

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'missing command: %s\n' "$command_name" >&2
    exit 1
  fi
}

for command_name in brew clang clang++ llvm-config lld ld.lld cargo rustc wild jq; do
  require_command "$command_name"
done

clang_major="$(clang --version | sed -n 's/.*version \([0-9][0-9]*\).*/\1/p' | head -n 1)"
if [[ "${clang_major}" != "${llvm_major}" ]]; then
  printf 'clang major version mismatch: expected %s, got %s\n' "${llvm_major}" "${clang_major:-unknown}" >&2
  exit 1
fi

llvm_config_major="$(llvm-config --version | cut -d. -f1)"
if [[ "${llvm_config_major}" != "${llvm_major}" ]]; then
  printf 'llvm-config major version mismatch: expected %s, got %s\n' "${llvm_major}" "${llvm_config_major:-unknown}" >&2
  exit 1
fi

wild_version="$(wild --version | head -n 1)"
if [[ "${wild_version}" != *"0.9.0"* ]]; then
  printf 'wild version mismatch: expected 0.9.0, got %s\n' "${wild_version}" >&2
  exit 1
fi

if [[ ! -f "${cargo_config}" ]]; then
  printf 'missing Cargo config: %s\n' "${cargo_config}" >&2
  exit 1
fi

grep -F '[target.aarch64-unknown-linux-gnu]' "${cargo_config}" >/dev/null
grep -F '[target.x86_64-unknown-linux-gnu]' "${cargo_config}" >/dev/null
grep -F 'linker = "clang"' "${cargo_config}" >/dev/null
grep -F 'rustflags = ["-Clink-arg=--ld-path=wild"]' "${cargo_config}" >/dev/null

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT
mkdir -p "${tmpdir}/src"
cat > "${tmpdir}/Cargo.toml" <<'EOF'
[package]
name = "agentics-toolchain-smoke"
version = "0.0.0"
edition = "2024"
publish = false
EOF
cat > "${tmpdir}/src/main.rs" <<'EOF'
fn main() {
    println!("agentics toolchain smoke");
}
EOF

CARGO_TARGET_DIR="${tmpdir}/target" cargo build --manifest-path "${tmpdir}/Cargo.toml" >/dev/null
"${tmpdir}/target/debug/agentics-toolchain-smoke" >/dev/null

printf 'Agentics Rust toolchain smoke check passed.\n'
printf '\nTool versions:\n'
brew --version | head -n 1
clang --version | head -n 1
llvm-config --version
cargo binstall --version
wild --version
rustc --version
cargo --version

printf '\nToolchain metadata:\n'
jq . /opt/agentics/toolchain-info.json
