#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
hooks_src="$repo_root/.githooks"
hooks_dst="$repo_root/.git/hooks"

if [[ ! -d "$hooks_src" ]]; then
  echo "install-git-hooks: missing $hooks_src" >&2
  exit 1
fi

if [[ ! -d "$hooks_dst" ]]; then
  echo "install-git-hooks: missing $hooks_dst (run inside a git repo)." >&2
  exit 1
fi

for hook in pre-commit pre-push; do
  src="$hooks_src/$hook"
  dst="$hooks_dst/$hook"
  if [[ ! -f "$src" ]]; then
    echo "install-git-hooks: skipping missing hook $src"
    continue
  fi
  cp "$src" "$dst"
  chmod +x "$dst"
  echo "Installed $hook -> $dst"
done

echo "Git hooks installed."

