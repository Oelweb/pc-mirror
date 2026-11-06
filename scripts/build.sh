#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/ucrt64/bin:$PATH"
cmake -S . -B build/app -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build/app --parallel 4
