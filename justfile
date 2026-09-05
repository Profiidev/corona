gpui_version := "0.3.3"
gpui_crates := "gpui-pre gpui-pre-linux"

dev: vendor
  cargo run

# The gpui crates with patches/<crate>/*.diff applied. Derived, so gitignored —
# zed's own crates can't be used directly, they need its entire workspace.
# `rm -rf vendor` to force a refetch after bumping gpui_version or a patch.
vendor:
  #!/usr/bin/env sh
  set -e
  rm -rf vendor
  mkdir -p vendor
  for c in {{gpui_crates}}; do
    curl -sL --fail "https://static.crates.io/crates/$c/{{gpui_version}}/download" | tar xz -C vendor
    mv "vendor/$c-{{gpui_version}}" "vendor/$c"
    for p in "$PWD/patches/$c"/*.diff; do
      [ -e "$p" ] || continue
      (cd "vendor/$c" && patch -p3 --forward < "$p")
    done
  done
