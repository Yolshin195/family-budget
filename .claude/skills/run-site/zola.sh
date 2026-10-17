#!/usr/bin/env bash
# Печатает путь к zola той же версии, что в CI (.github/workflows/site.yml).
# При первом запуске скачивает релиз с GitHub и сверяет sha256 оттуда же; дальше берёт из кэша.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
wf="$root/.github/workflows/site.yml"
ver=$(sed -n 's/^ *ZOLA_VERSION: *//p' "$wf" | head -1)
sha=$(sed -n 's/^ *ZOLA_SHA256: *//p' "$wf" | head -1)
if command -v zola >/dev/null && [ "$(zola --version)" = "zola $ver" ]; then
  command -v zola
  exit 0
fi
dir="${XDG_CACHE_HOME:-$HOME/.cache}/family-budget/zola-$ver"
if [ ! -x "$dir/zola" ]; then
  mkdir -p "$dir"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL -o "$tmp/zola.tgz" \
    "https://github.com/getzola/zola/releases/download/v$ver/zola-v$ver-x86_64-unknown-linux-gnu.tar.gz"
  echo "$sha  $tmp/zola.tgz" | sha256sum -c - >&2
  tar -xzf "$tmp/zola.tgz" -C "$dir"
fi
echo "$dir/zola"
