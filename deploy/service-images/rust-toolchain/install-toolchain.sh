#!/usr/bin/env bash
set -euo pipefail

llvm_major="${LLVM_MAJOR:-22}"
wild_linker_version="${WILD_LINKER_VERSION:-0.9.0}"
rust_base_image="${RUST_BASE_IMAGE:-unknown}"

if [[ "$(id -u)" -ne 0 ]]; then
  printf 'install-toolchain.sh must run as root inside the image build.\n' >&2
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive
export HOMEBREW_PREFIX=/home/linuxbrew/.linuxbrew
export HOMEBREW_NO_ANALYTICS=1
export CARGO_HOME=/opt/cargo
export PATH="${HOMEBREW_PREFIX}/opt/llvm/bin:${HOMEBREW_PREFIX}/bin:${HOMEBREW_PREFIX}/sbin:${CARGO_HOME}/bin:/usr/local/cargo/bin:${PATH}"

apt-get update
apt-get install -y --no-install-recommends \
  build-essential \
  ca-certificates \
  curl \
  file \
  git \
  jq \
  procps
rm -rf /var/lib/apt/lists/*

if ! id linuxbrew >/dev/null 2>&1; then
  useradd --create-home --home-dir /home/linuxbrew --shell /bin/bash linuxbrew
fi
install -d -m 0755 -o linuxbrew -g linuxbrew "${HOMEBREW_PREFIX}"

if [[ ! -x "${HOMEBREW_PREFIX}/bin/brew" ]]; then
  su linuxbrew -c 'NONINTERACTIVE=1 CI=1 HOMEBREW_NO_ANALYTICS=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
fi

su linuxbrew -c "HOMEBREW_NO_ANALYTICS=1 ${HOMEBREW_PREFIX}/bin/brew install llvm cargo-binstall"

install -d -m 0755 "${CARGO_HOME}"
cargo binstall --no-confirm "wild-linker@${wild_linker_version}"

cat > "${CARGO_HOME}/config.toml" <<'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-Clink-arg=--ld-path=wild"]

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-Clink-arg=--ld-path=wild"]
EOF

chmod -R a+rX "${CARGO_HOME}"

clang_major="$(clang --version | sed -n 's/.*version \([0-9][0-9]*\).*/\1/p' | head -n 1)"
if [[ "${clang_major}" != "${llvm_major}" ]]; then
  printf 'Homebrew llvm major mismatch: expected %s, got %s\n' "${llvm_major}" "${clang_major:-unknown}" >&2
  exit 1
fi

install -d -m 0755 /opt/agentics
cat > /opt/agentics/toolchain-info.json <<EOF
{
  "rust_base_image": "${rust_base_image}",
  "homebrew_prefix": "${HOMEBREW_PREFIX}",
  "llvm_major": "${llvm_major}",
  "clang_version": "$(clang --version | head -n 1)",
  "llvm_config_version": "$(llvm-config --version)",
  "cargo_binstall_version": "$(cargo binstall --version | head -n 1)",
  "wild_version": "$(wild --version | head -n 1)",
  "rustc_version": "$(rustc --version)",
  "cargo_version": "$(cargo --version)",
  "cargo_home": "${CARGO_HOME}",
  "cargo_config": "${CARGO_HOME}/config.toml"
}
EOF
