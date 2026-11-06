#!/usr/bin/env bash
# Baut den UxPlay-Empfänger im MSYS2-UCRT64-Terminal nach build/backend/bin/uxplay.exe.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/ucrt64/bin:$PATH"
cmake -S . -B build/backend -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build/backend --parallel 4
ctest --test-dir build/backend --output-on-failure
