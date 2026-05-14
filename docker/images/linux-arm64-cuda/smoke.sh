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
  apt-fast aria2c jq file less nano vim.tiny time tini nvcc; do
  require_command "$command_name"
done

if [[ "${AGENTICS_GPU_SMOKE_REQUIRE_DEVICE:-0}" == "1" ]]; then
  require_command nvidia-smi
fi

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

printf 'Agentics CUDA base smoke check passed.\n'
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
nvcc --version | tail -n 1
tini --version

if [[ "${AGENTICS_GPU_SMOKE_REQUIRE_DEVICE:-0}" == "1" ]]; then
  printf '\nGPU devices:\n'
  nvidia-smi -L

  cat > /tmp/agentics-cuda-smoke.cu <<'CUDA'
#include <cuda_runtime.h>

#include <cstdio>

__global__ void fill_answer(int* out) {
  out[0] = 42;
}

int main() {
  int device_count = 0;
  cudaError_t status = cudaGetDeviceCount(&device_count);
  if (status != cudaSuccess || device_count < 1) {
    std::fprintf(stderr, "cudaGetDeviceCount failed: %s\n", cudaGetErrorString(status));
    return 1;
  }

  int* device_value = nullptr;
  status = cudaMalloc(&device_value, sizeof(int));
  if (status != cudaSuccess) {
    std::fprintf(stderr, "cudaMalloc failed: %s\n", cudaGetErrorString(status));
    return 1;
  }

  fill_answer<<<1, 1>>>(device_value);
  status = cudaDeviceSynchronize();
  if (status != cudaSuccess) {
    std::fprintf(stderr, "kernel launch failed: %s\n", cudaGetErrorString(status));
    cudaFree(device_value);
    return 1;
  }

  int host_value = 0;
  status = cudaMemcpy(&host_value, device_value, sizeof(int), cudaMemcpyDeviceToHost);
  cudaFree(device_value);
  if (status != cudaSuccess || host_value != 42) {
    std::fprintf(stderr, "cudaMemcpy or result check failed: %s, value=%d\n", cudaGetErrorString(status), host_value);
    return 1;
  }

  std::printf("CUDA runtime smoke passed with %d device(s).\n", device_count);
  return 0;
}
CUDA
  nvcc -std=c++17 /tmp/agentics-cuda-smoke.cu -o /tmp/agentics-cuda-smoke
  /tmp/agentics-cuda-smoke
else
  printf '\nGPU runtime probe skipped. Set AGENTICS_GPU_SMOKE_REQUIRE_DEVICE=1 and run with GPU access to require it.\n'
fi

printf '\nImage metadata:\n'
cat /opt/agentics/image-info.json
printf '\n'
