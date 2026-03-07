# Shirase home-manager module — notification center with typed config + daemon
#
# Namespace: blackmatter.components.shirase.*
#
# Generates YAML config from typed Nix options, loaded by shikumi at runtime.
# Supports hot-reload via symlink-aware file watching.
#
# Module factory: receives { hmHelpers } from flake.nix, returns HM module.
{ hmHelpers }:
{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
let
  inherit (hmHelpers) mkLaunchdService mkSystemdService;
  cfg = config.blackmatter.components.shirase;
  isDarwin = pkgs.stdenv.isDarwin;

  logDir =
    if isDarwin then "${config.home.homeDirectory}/Library/Logs"
    else "${config.home.homeDirectory}/.local/share/shirase/logs";

  # -- YAML config generation --------------------------------------------------
  settingsAttr = let
    appearance = filterAttrs (_: v: v != null) {
      inherit (cfg.appearance) width max_visible opacity position animation_ms;
    };

    behavior = filterAttrs (_: v: v != null) {
      inherit (cfg.behavior) auto_dismiss_secs do_not_disturb group_by_app sound_enabled;
    };

    filters = filterAttrs (_: v: v != null) {
      blocked_apps = if cfg.filters.blocked_apps == [] then null else cfg.filters.blocked_apps;
      priority_apps = if cfg.filters.priority_apps == [] then null else cfg.filters.priority_apps;
    } // optionalAttrs (cfg.filters.quiet_hours.start != null) {
      quiet_hours = {
        inherit (cfg.filters.quiet_hours) start end;
      };
    };

    history = filterAttrs (_: v: v != null) {
      inherit (cfg.history) max_entries retention_days;
    };

    daemon = optionalAttrs cfg.daemon.enable (filterAttrs (_: v: v != null) {
      listen_addr = cfg.daemon.listen_addr;
      socket_path = cfg.daemon.socket_path;
    });
  in
    filterAttrs (_: v: v != {} && v != null) {
      inherit appearance behavior filters history daemon;
    }
    // cfg.extraSettings;

  yamlConfig = pkgs.writeText "shirase.yaml"
    (lib.generators.toYAML { } settingsAttr);
in
{
  options.blackmatter.components.shirase = {
    enable = mkEnableOption "Shirase — notification center";

    package = mkOption {
      type = types.package;
      default = pkgs.shirase;
      description = "The shirase package to use.";
    };

    # -- Appearance ------------------------------------------------------------
    appearance = {
      width = mkOption {
        type = types.int;
        default = 360;
        description = "Notification popup width in pixels.";
      };

      max_visible = mkOption {
        type = types.int;
        default = 5;
        description = "Maximum number of visible notifications at once.";
      };

      opacity = mkOption {
        type = types.float;
        default = 0.95;
        description = "Notification opacity (0.0-1.0).";
      };

      position = mkOption {
        type = types.enum [ "top-right" "top-left" "bottom-right" ];
        default = "top-right";
        description = "Screen position for notification popups.";
      };

      animation_ms = mkOption {
        type = types.int;
        default = 200;
        description = "Animation duration in milliseconds.";
      };
    };

    # -- Behavior --------------------------------------------------------------
    behavior = {
      auto_dismiss_secs = mkOption {
        type = types.int;
        default = 5;
        description = "Seconds before auto-dismissing notifications.";
      };

      do_not_disturb = mkOption {
        type = types.bool;
        default = false;
        description = "Enable do-not-disturb mode by default.";
      };

      group_by_app = mkOption {
        type = types.bool;
        default = true;
        description = "Group notifications by source application.";
      };

      sound_enabled = mkOption {
        type = types.bool;
        default = true;
        description = "Enable notification sounds.";
      };
    };

    # -- Filters ---------------------------------------------------------------
    filters = {
      blocked_apps = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Apps whose notifications are blocked.";
        example = [ "com.apple.Chess" "Automator" ];
      };

      priority_apps = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Apps whose notifications are always shown (even in DND).";
        example = [ "com.apple.Messages" "Slack" ];
      };

      quiet_hours = {
        start = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Quiet hours start time (e.g. \"22:00\"). Null to disable.";
        };

        end = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Quiet hours end time (e.g. \"07:00\"). Null to disable.";
        };
      };
    };

    # -- History ---------------------------------------------------------------
    history = {
      max_entries = mkOption {
        type = types.int;
        default = 1000;
        description = "Maximum number of notification history entries.";
      };

      retention_days = mkOption {
        type = types.int;
        default = 30;
        description = "Days to retain notification history.";
      };
    };

    # -- Daemon ----------------------------------------------------------------
    daemon = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Run shirase as a persistent daemon (launchd on macOS, systemd on Linux).
          The daemon listens for system notifications and manages the notification queue.
        '';
      };

      listen_addr = mkOption {
        type = types.str;
        default = "0.0.0.0:50053";
        description = "Listen address for the daemon.";
      };

      socket_path = mkOption {
        type = types.str;
        default = "/tmp/shirase.sock";
        description = "Unix socket path for local IPC.";
      };
    };

    # -- Escape hatch ----------------------------------------------------------
    extraSettings = mkOption {
      type = types.attrs;
      default = {};
      description = ''
        Additional raw settings merged on top of typed options.
        Use this for experimental or newly-added config keys not yet
        covered by typed options. Values are serialized directly to YAML.
      '';
    };
  };

  config = mkIf cfg.enable (mkMerge [
    # Install the package
    {
      home.packages = [ cfg.package ];
    }

    # Create log directory
    {
      home.activation.shirase-log-dir = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        run mkdir -p "${logDir}"
      '';
    }

    # YAML configuration -- always generated from typed options
    {
      xdg.configFile."shirase/shirase.yaml".source = yamlConfig;
    }

    # Darwin: launchd agent (daemon mode)
    (mkIf (cfg.daemon.enable && isDarwin)
      (mkLaunchdService {
        name = "shirase";
        label = "io.pleme.shirase";
        command = "${cfg.package}/bin/shirase";
        args = [ "daemon" ];
        logDir = logDir;
        processType = "Interactive";
        keepAlive = true;
      })
    )

    # Linux: systemd user service (daemon mode)
    (mkIf (cfg.daemon.enable && !isDarwin)
      (mkSystemdService {
        name = "shirase";
        description = "Shirase — notification center daemon";
        command = "${cfg.package}/bin/shirase";
        args = [ "daemon" ];
      })
    )
  ]);
}
