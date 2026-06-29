#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: scripts/restore_real_hftbt.sh /path/to/uploaded/hftbt.rs" >&2
  exit 2
fi

SRC="$1"

if [ ! -f "$SRC" ]; then
  echo "source file not found: $SRC" >&2
  exit 2
fi

cp "$SRC" hftbt.rs
rustc --edition=2024 hftbt.rs -o /tmp/hftbt
/tmp/hftbt --help >/tmp/hftbt_help.txt
/tmp/hftbt harness

git add hftbt.rs
git commit -m "restore real fused hftbt single-file appliance"

echo "restored and committed real hftbt.rs"
echo "next: git push"
