gpui_version := "0.3.3"

dev: vendor
  cargo run

# gpui-pre-linux with patches/*.diff applied. Derived, so it's gitignored — zed's
# own crates/gpui_linux can't be used directly, it needs its entire workspace.
# `rm -rf vendor` to force a refetch after bumping gpui_version or editing a patch.
vendor:
  #!/usr/bin/env sh
  set -e
  rm -rf vendor
  mkdir -p vendor
  curl -sL --fail https://static.crates.io/crates/gpui-pre-linux/{{gpui_version}}/download | tar xz -C vendor
  mv vendor/gpui-pre-linux-{{gpui_version}} vendor/gpui-pre-linux
  cd vendor/gpui-pre-linux
  for p in ../../patches/*.diff; do patch -p3 --forward < "$p"; done
