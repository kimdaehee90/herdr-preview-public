#!/bin/bash
# Use a local, authenticated Git clone for both public and private repositories.
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
command -v python3 >/dev/null || { echo 'Python 3 is required (Xcode Command Line Tools includes it).' >&2; exit 1; }
exec python3 "$repo_dir/distribution/restore-personal.py" "$@"
