#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/ucrt64/bin:$PATH"
cmake -S third_party/uxplay -B build/backend -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DNO_MARCH_NATIVE=ON -DUSE_MDNS=ON -DUSE_DNS_SD=OFF
cmake --build build/backend --parallel 4
