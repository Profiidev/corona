{
  config,
  lib,
  modulesPath,
  pkgs,
  ...
}:

let
  hyprlandConf = pkgs.writeText "hyprland.conf" ''
    monitor = , preferred, auto, 1
    misc {
      disable_hyprland_logo = true
      disable_splash_rendering = true
    }
    cursor {
      no_hardware_cursors = true # VM
    }
    bind = SUPER, Return, exec, ${pkgs.foot}/bin/foot
    bind = SUPER, Q, killactive
    bind = SUPER SHIFT, E, exit
  '';

  hyprlandVm =
    (pkgs.writeTextDir "share/wayland-sessions/hyprland-vm.desktop" ''
      [Desktop Entry]
      Name=Hyprland (VM)
      Exec=${config.programs.hyprland.package}/bin/start-hyprland -- --config ${hyprlandConf}
      DesktopNames=Hyprland
      Type=Application
    '').overrideAttrs
      { passthru.providedSessions = [ "hyprland-vm" ]; };
in
{
  imports = [ "${modulesPath}/virtualisation/qemu-vm.nix" ];

  boot = {
    # Silent boot: no kernel/systemd text or cursor between splash and greeter.
    consoleLogLevel = 0;
    initrd.verbose = false;
    kernelParams = [
      "quiet"
      "splash"
      "loglevel=3"
      "rd.systemd.show_status=auto"
      "rd.udev.log_priority=3"
      "vt.global_cursor_default=0"
      # qemu-vm adds console=ttyS0; Plymouth would otherwise pick the text splash.
      "plymouth.ignore-serial-consoles"
    ];
    initrd.systemd.enable = true;
    # Early KMS. VM GPU; on real hardware use i915/xe/amdgpu/nvidia.
    initrd.kernelModules = [ "virtio_gpu" ];

    plymouth.enable = true;
  };

  services.greetd = {
    enable = true;
    settings.default_session.command = lib.concatStringsSep " " [
      "${pkgs.systemd}/bin/systemd-cat -t greeter"
      "${pkgs.coreutils}/bin/env"
      "VK_DRIVER_FILES=${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json"
      "LD_LIBRARY_PATH=${
        lib.makeLibraryPath [
          pkgs.wayland
          pkgs.vulkan-loader
        ]
      }"
      "${pkgs.cage}/bin/cage -s -- /mnt/corona/target/debug/corona_greeter"
    ];
    # GDM-style handoff, see systemd.services.greetd below
    greeterManagesPlymouth = true;
  };

  systemd.services.greetd = {
    conflicts = [ "plymouth-quit.service" ];
    after = [ "plymouth-quit.service" ];
    serviceConfig = {
      # idle would wait (up to 5s) for plymouth-quit-wait, which only ends when we quit Plymouth.
      Type = lib.mkForce "simple";
      ExecStartPre = "-${pkgs.plymouth}/bin/plymouth deactivate";
      # Quit only after cage drew: closing Plymouth's DRM fd turns off a CRTC still showing
      # its framebuffer. ponytail: fixed delay; a greeter-side signal would be exact.
      ExecStartPost = "-${pkgs.bash}/bin/sh -c 'sleep 3; ${pkgs.plymouth}/bin/plymouth quit --retain-splash'";
    };
  };

  programs.hyprland.enable = true;
  programs.hyprland.withUWSM = true;
  services.displayManager.sessionPackages = [ hyprlandVm ];

  users.users.test = {
    isNormalUser = true;
    password = "test";
  };
  users.users.root.password = "root"; # to read journalctl -t greeter

  virtualisation.sharedDirectories.corona = {
    source = ''$(git -C "$OLDPWD" rev-parse --show-toplevel 2>/dev/null || echo "$OLDPWD")'';
    target = "/mnt/corona";
  };
  virtualisation.useEFIBoot = true;
  virtualisation.memorySize = 2048;
  system.stateVersion = "25.11";
}
