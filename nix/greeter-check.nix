{
  self,
  pkgs,
  lib,
}:

let
  command =
    modules:
    let
      vm = self.nixosConfigurations.greeter-vm.extendModules { inherit modules; };
      cmd = vm.config.services.greetd.settings.default_session.command;
    in
    {
      # only the script needs building, not cage and the rest
      env = builtins.unsafeDiscardStringContext cmd;
      # match drops the context that builds it, so take it back from the command
      script = builtins.appendContext (builtins.head (builtins.match ".* -- ([^ ]*)" cmd)) {
        ${
          lib.findFirst (lib.hasSuffix "-corona-greeter.drv") null (
            builtins.attrNames (builtins.getContext cmd)
          )
        } =
          {
            outputs = [ "out" ];
          };
      };
    };

  configured = command [
    {
      services.corona-greeter = {
        outputs = {
          eDP-1.position = "2560,0";
          DP-4.position = "0,0";
        };
        keyboard = {
          layout = "de,us";
          variant = "";
          options = "grp:alt_shift_toggle";
        };
      };
    }
  ];
  plain = command [ ];
in
pkgs.runCommand "greeter-module-check" { } ''
  set -eu
  has() { grep -qF -- "$2" "$1" || { echo "missing in $1: $2"; exit 1; }; }
  lacks() { ! grep -qF -- "$2" "$1" || { echo "unexpected in $1: $2"; exit 1; }; }

  echo ${lib.escapeShellArg configured.env} > configured
  has configured "XKB_DEFAULT_LAYOUT=de,us"
  has configured "XKB_DEFAULT_OPTIONS=grp:alt_shift_toggle"
  # empty ones are left to xkbcommon
  lacks configured "XKB_DEFAULT_VARIANT"

  has ${configured.script} "--output DP-4 --pos 0,0 || true"
  has ${configured.script} "--output eDP-1 --pos 2560,0 || true"
  # the greeter starts after the layout, replacing the script
  tail -n 2 ${configured.script} | grep -q "^exec .*corona_greeter$"

  lacks ${plain.script} "wlr-randr"
  has ${plain.script} "exec "
  touch $out
''
