#!/usr/bin/env bash
set -euo pipefail

nix_file="${1:?Usage: $0 <nix-file>}"
is_fork="${IS_FORK:-false}"

set_output() {
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    echo "$1" >> "$GITHUB_OUTPUT"
  fi
}

set +e
output=$(nix build .#default.cargoDeps --no-link --accept-flake-config 2>&1)
status=$?
set -e

if [ "$status" -eq 0 ]; then
  echo "cargoHash is up to date."
  set_output "changed=false"
  exit 0
fi

new_hash=$(printf '%s\n' "$output" | grep -oE 'got:\s+sha256-[A-Za-z0-9+/=]+' | tail -n1 | awk '{print $2}')
if [ -z "$new_hash" ]; then
  echo "$output"
  echo "::error::Failed to determine updated cargoHash from nix build output."
  exit 1
fi

if [ "$is_fork" = "true" ]; then
  echo "::error::cargoHash in $nix_file is out of date. Please update it to:"
  echo "  cargoHash = \"$new_hash\";"
  exit 1
fi

sed -i -E "s|cargoHash = [^;]+;|cargoHash = \"$new_hash\";|" "$nix_file"
nix build .#default.cargoDeps --no-link --accept-flake-config
echo "Updated cargoHash in $nix_file to $new_hash"
set_output "changed=true"
