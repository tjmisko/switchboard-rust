#!/usr/bin/env bash
# Render the guide: the numbered fragments in name order become one HTML file.
# Usage: exercises/guide/build.sh [output path]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="${1:-$HOME/Guides/Building Switchboard in Rust.html}"
cat "$here"/[1-9]*.html >| "$out"
echo "wrote $out"
