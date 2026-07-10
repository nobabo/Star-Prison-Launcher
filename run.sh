#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export PATH="/mnt/c/Program Files/nodejs:/mnt/c/Users/82105/.cargo/bin:$PATH"

cd "$ROOT_DIR"

if [ ! -d node_modules ]; then
    pnpm install
fi

pnpm dev
