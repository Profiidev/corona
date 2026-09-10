gpui_version := "0.3.4"
gpui_crates := "gpui-pre gpui-pre-linux"

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

# Run corona in a nested Hyprland — a lock screen that won't unlock can't lock you out
nested:
  #!/usr/bin/env sh
  set -e
  # None of ~/.config/hypr is loaded, so nothing here can touch the real session.
  # SUPER+SHIFT+Q quits the nested one; so does Ctrl-C here, or `just nested-kill`.
  cargo build
  conf="$(mktemp -t corona-nested-XXXXXX.lua)"
  trap 'rm -f "$conf"' EXIT
  cat > "$conf" <<LUA
  -- Hyprland does not hand its own environment to the processes it execs, so the
  -- library path devenv sets has to be passed in or corona dies with NoWaylandLib.
  hl.env("LD_LIBRARY_PATH", "{{env('LD_LIBRARY_PATH', '')}}")

  -- No pinned mode: the wayland backend's output then follows its host window,
  -- so resizing the nested session's window resizes the screen inside it.
  hl.monitor({
      output = "",
      mode = "preferred",
      position = "auto",
      scale = "1",
  })

  hl.config({
      misc = {
          disable_hyprland_logo = true,
          disable_splash_rendering = true,
          force_default_wallpaper = 0,
      },
      -- Off, so what you are looking at is corona and not the compositor.
      animations = {
          enabled = false,
      },
      decoration = {
          blur = {
              enabled = false,
          },
      },
  })

  -- The way out if the shell wedges. Goes through hyprctl rather than a dispatcher
  -- name, since hyprctl targets this nested instance and needs no guessing.
  hl.bind("SUPER + SHIFT + Q", hl.dsp.exec_cmd("hyprctl dispatch exit"))

  -- Redirected: an exec'd child's output does not reach the terminal running this.
  hl.on("hyprland.start", function()
      hl.exec_cmd("sh -c '{{justfile_directory()}}/target/debug/corona >/tmp/corona-nested.log 2>&1'")
  end)
  LUA
  echo "nested session starting — SUPER+SHIFT+Q to quit; corona logs to /tmp/corona-nested.log"
  # Killing the watchdog alone leaves Hyprland running, so nested-kill matches on
  # the config path instead — both processes carry it in argv.
  echo "$conf" > /tmp/corona-nested.conf-path
  # Through start-hyprland: launching Hyprland directly only warns, but this is
  # what it asks for. Everything after -- goes to Hyprland itself. Not exec: that
  # would discard the EXIT trap and leak the temp config.
  start-hyprland -- -c "$conf"

# Print the nested session's HYPRLAND_INSTANCE_SIGNATURE.
[private]
nested-sig:
  #!/usr/bin/env sh
  set -e
  # `|| true`, or `set -e` kills the script before the message below.
  conf="$(cat /tmp/corona-nested.conf-path 2>/dev/null || true)"
  if [ -z "$conf" ]; then
    echo "no nested session running" >&2
    exit 1
  fi
  pid="$(pgrep -f "Hyprland .*-c $conf" | head -1 || true)"
  if [ -z "$pid" ]; then
    echo "nested session not running" >&2
    exit 1
  fi
  # The instance whose pid is the nested compositor's. `hyprctl -i` takes an
  # index into this same list instead, and nothing promises the nested session
  # keeps its place there.
  sig="$(hyprctl instances | awk -v pid="$pid" '/^instance /{ s = $2 } /^\tpid: /{ if ($2 == pid) { sub(/:$/, "", s); print s; exit } }')"
  if [ -z "$sig" ]; then
    echo "nested session has no hyprland instance" >&2
    exit 1
  fi
  echo "$sig"

# Run hyprctl against the nested session, e.g. `just nested-ctl -j monitors`.
nested-ctl *args:
  #!/usr/bin/env sh
  set -e
  HYPRLAND_INSTANCE_SIGNATURE="$({{just_executable()}} nested-sig)" hyprctl {{args}}

# Add a second output to the nested session, right of the first one.
nested-monitor:
  #!/usr/bin/env sh
  set -e
  export HYPRLAND_INSTANCE_SIGNATURE="$({{just_executable()}} nested-sig)"
  before="$(hyprctl monitors | awk '/^Monitor /{ print $2 }')"
  hyprctl output create wayland
  # The output is announced asynchronously, so it is not in the list yet.
  for _ in $(seq 20); do
    name="$(hyprctl monitors | awk '/^Monitor /{ print $2 }' | grep -vxF "$before" | head -1 || true)"
    [ -n "$name" ] && break
    sleep 0.2
  done
  if [ -z "$name" ]; then
    echo "no new output appeared"
    exit 1
  fi
  # Lua, because `hyprctl keyword` answers "unknown request" on this Hyprland.
  # `auto-right` and `preferred` both track the host windows as they are resized,
  # where a pinned mode and an x offset would go stale on the first resize.
  hyprctl eval "hl.monitor({ output = \"$name\", mode = \"preferred\", position = \"auto-right\", scale = 1 })"
  echo "added $name right of the nested screen"

# Kill a nested session started by `just nested`.
nested-kill:
  #!/usr/bin/env sh
  # The expanded path only ever reaches pkill's own argv, and pkill never matches
  # itself — so this cannot take down the shell running it.
  conf="$(cat /tmp/corona-nested.conf-path 2>/dev/null)"
  if [ -z "$conf" ]; then
    echo "no nested session running"
    exit 0
  fi
  # The watchdog goes first and hard: start-hyprland restarts the compositor it
  # supervises, so killing them together just spawns a fresh one.
  pkill -9 -f "start-hyprland -- -c $conf" 2>/dev/null || true
  sleep 1
  pkill -9 -f "Hyprland .*-c $conf" 2>/dev/null || true
  rm -f /tmp/corona-nested.conf-path
  echo "killed nested session"
