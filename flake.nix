{
  description = "Shirase (知らせ) — notification center for macOS and Linux";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
  }:
    (import "${substrate}/lib/rust-tool-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "shirase";
      src = self;
      repo = "pleme-io/shirase";

      # Migration to substrate module-trio + shikumiTypedGroups.
      # Typed groups for appearance/behavior/filters/history,
      # withUserDaemon for the `shirase daemon` notification listener
      # (Interactive priority on Darwin), withShikumiConfig for the
      # YAML config at ~/.config/shirase/shirase.yaml.
      #
      # Note: the legacy module's daemon.{listen_addr,socket_path} fields
      # are moved into a sibling typed group `daemon_settings` because the
      # trio's withUserDaemon owns daemon.{enable,extraArgs,environment}.
      # daemon_settings serializes into the YAML at the legacy `daemon`
      # key. Interactive process type is achieved via extraHmConfigFn
      # bypass since the trio's withUserDaemon hard-codes the standard
      # priority.
      module = {
        description = "Shirase (知らせ) — notification center";
        hmNamespace = "blackmatter.components";

        # No withUserDaemon — Interactive processType requires a custom
        # launchd entry. The daemon is wired via extraHmConfigFn below
        # to preserve parity with the legacy module.

        # Shikumi YAML config at ~/.config/shirase/shirase.yaml.
        withShikumiConfig = true;

        shikumiTypedGroups = {
          appearance = {
            width        = { type = "int";   default = 360;  description = "Notification popup width in pixels."; };
            max_visible  = { type = "int";   default = 5;    description = "Maximum number of visible notifications at once."; };
            opacity      = { type = "float"; default = 0.95; description = "Notification opacity (0.0-1.0)."; };
            position     = {
              type = nixpkgs.lib.types.enum [ "top-right" "top-left" "bottom-right" ];
              default = "top-right";
              description = "Screen position for notification popups.";
            };
            animation_ms = { type = "int"; default = 200; description = "Animation duration in milliseconds."; };
          };

          behavior = {
            auto_dismiss_secs = { type = "int";  default = 5;     description = "Seconds before auto-dismissing notifications."; };
            do_not_disturb    = { type = "bool"; default = false; description = "Enable do-not-disturb mode by default."; };
            group_by_app      = { type = "bool"; default = true;  description = "Group notifications by source application."; };
            sound_enabled     = { type = "bool"; default = true;  description = "Enable notification sounds."; };
          };

          filters = {
            blocked_apps = {
              type = "listOfStr";
              default = [ ];
              description = "Apps whose notifications are blocked.";
            };
            priority_apps = {
              type = "listOfStr";
              default = [ ];
              description = "Apps whose notifications are always shown (even in DND).";
            };
          };

          history = {
            max_entries    = { type = "int"; default = 1000; description = "Maximum number of notification history entries."; };
            retention_days = { type = "int"; default = 30;   description = "Days to retain notification history."; };
          };
        };

        # Bespoke options:
        #   - daemon submodule (enable + listen_addr + socket_path) since
        #     withUserDaemon would lock processType to Adaptive.
        #   - quiet_hours (nested submodule under filters.quiet_hours)
        #     conditionally serialized only when both fields set.
        #   - extraSettings escape hatch.
        extraHmOptions = {
          daemon = {
            enable = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.bool;
              default = false;
              description = ''
                Run shirase as a persistent daemon (launchd on macOS,
                systemd on Linux). The daemon listens for system
                notifications and manages the notification queue.
              '';
            };
            listen_addr = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.str;
              default = "0.0.0.0:50053";
              description = "Listen address for the daemon.";
            };
            socket_path = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.str;
              default = "/tmp/shirase.sock";
              description = "Unix socket path for local IPC.";
            };
          };
          quiet_hours = {
            start = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Quiet hours start time (e.g. \"22:00\"). Null to disable.";
            };
            end = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Quiet hours end time (e.g. \"07:00\"). Null to disable.";
            };
          };
          extraSettings = nixpkgs.lib.mkOption {
            type = nixpkgs.lib.types.attrs;
            default = { };
            description = "Additional raw settings merged on top of the typed YAML.";
          };
        };

        # Custom daemon wiring + YAML extras. Daemon uses `Interactive`
        # processType (notification listener needs UI thread on macOS);
        # also merges daemon settings + quiet_hours into the YAML at the
        # legacy keys.
        extraHmConfigFn = { cfg, pkgs, lib, config, ... }:
          let
            hmHelpers = import "${substrate}/lib/hm/service-helpers.nix" {
              inherit lib;
            };
            isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
            logDir =
              if isDarwin then "${config.home.homeDirectory}/Library/Logs"
              else "${config.home.homeDirectory}/.local/share/shirase/logs";

            daemonExtras =
              if cfg.daemon.enable
              then {
                daemon = {
                  listen_addr = cfg.daemon.listen_addr;
                  socket_path = cfg.daemon.socket_path;
                };
              }
              else { };

            # Quiet hours merge into filters.quiet_hours when both set.
            quietHoursExtras =
              if cfg.quiet_hours.start != null && cfg.quiet_hours.end != null
              then {
                filters = {
                  quiet_hours = {
                    start = cfg.quiet_hours.start;
                    end = cfg.quiet_hours.end;
                  };
                };
              }
              else { };

            extras = lib.recursiveUpdate
              (lib.recursiveUpdate daemonExtras quietHoursExtras)
              cfg.extraSettings;
          in lib.mkMerge [
            (lib.mkIf (extras != { }) {
              services.shirase.settings = extras;
            })

            {
              home.activation.shirase-log-dir =
                lib.hm.dag.entryAfter [ "writeBoundary" ] ''
                  run mkdir -p "${logDir}"
                '';
            }

            # Darwin: launchd agent with Interactive processType.
            (lib.mkIf (cfg.daemon.enable && isDarwin)
              (hmHelpers.mkLaunchdService {
                name = "shirase";
                label = "io.pleme.shirase";
                command = "${cfg.package}/bin/shirase";
                args = [ "daemon" ];
                logDir = logDir;
                processType = "Interactive";
                keepAlive = true;
              }))

            # Linux: systemd user service.
            (lib.mkIf (cfg.daemon.enable && !isDarwin)
              (hmHelpers.mkSystemdService {
                name = "shirase";
                description = "Shirase — notification center daemon";
                command = "${cfg.package}/bin/shirase";
                args = [ "daemon" ];
              }))
          ];
      };
    };
}
