#!/usr/bin/env bash
set -euo pipefail

missing=0

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'missing command: %s\n' "$command_name" >&2
    missing=1
  fi
}

for command_name in \
  bash dash sha256sum find grep sed awk tar gzip zip unzip \
  curl wget git gcc g++ make cmake pkg-config ninja \
  python python3 uv fnm node npm npx bun bunx rustup rustc cargo \
  apt-fast aria2c jq file less nano vim.tiny time tini; do
  require_command "$command_name"
done

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

printf 'Agentics CPU base smoke check passed.\n'
printf '\nTool versions:\n'
bash --version | head -n 1
python --version
uv --version
fnm --version
node --version
npm --version
bun --version
rustc --version
cargo --version
gcc --version | head -n 1
cmake --version | head -n 1
ninja --version
apt-fast --version 2>/dev/null || apt-fast --help | head -n 1
aria2c --version | head -n 1
tini --version

printf '\nImage metadata:\n'
cat /opt/agentics/image-info.json
printf '\n'
