self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.geosolver;
  inherit (lib)
    mkEnableOption
    mkIf
    mkOption
    types
    optional
    optionalAttrs
    ;

  secretsDir = "/var/lib/geosolver-secrets";
  envFile = "${secretsDir}/env";
  claudeEnv = "${secretsDir}/claude.env";
  stateDir = "/var/lib/geosolver";
  aiOn = cfg.ai.package != null;
  claudeBin = lib.getExe cfg.ai.package;
  url =
    if cfg.funnel.enable then
      "https://${cfg.publicHost}:${toString cfg.funnel.port}"
    else if cfg.publicHost != null then
      "https://${cfg.publicHost}"
    else
      "http://${cfg.address}:${toString cfg.port}";

  sandbox = {
    DynamicUser = true;
    User = "geosolver";
    Group = "geosolver";
    StateDirectory = "geosolver";
    StateDirectoryMode = "0700";
    WorkingDirectory = stateDir;
    UMask = "0077";
    EnvironmentFile = [ envFile ] ++ optional aiOn "-${claudeEnv}" ++ cfg.environmentFiles;
    NoNewPrivileges = true;
    ProtectSystem = "strict";
    ProtectHome = true;
    PrivateTmp = true;
    PrivateDevices = true;
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectKernelLogs = true;
    ProtectControlGroups = true;
    ProtectClock = true;
    ProtectHostname = true;
    ProtectProc = "invisible";
    RestrictAddressFamilies = [
      "AF_INET"
      "AF_INET6"
      "AF_UNIX"
    ];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    LockPersonality = true;
    MemoryDenyWriteExecute = !aiOn;
    SystemCallArchitectures = "native";
    SystemCallFilter = [ "@system-service" ];
    SystemCallErrorNumber = "EPERM";
    CapabilityBoundingSet = "";
    AmbientCapabilities = "";
  };

  aiEnv = optionalAttrs aiOn {
    HOME = stateDir;
    CLAUDE_CONFIG_DIR = "${stateDir}/claude";
    AGSTUDIO_CLAUDE_BIN = claudeBin;
    DISABLE_AUTOUPDATER = "1";
  };

  geosolver-password = pkgs.writeShellApplication {
    name = "geosolver-password";
    runtimeInputs = [
      pkgs.openssl
      pkgs.coreutils
      pkgs.gnused
      pkgs.systemd
    ];
    text = ''
      if [ "$(id -u)" -ne 0 ]; then
        echo "run as root: sudo geosolver-password [--rotate]" >&2
        exit 1
      fi
      if [ "''${1:-}" = "--rotate" ]; then
        umask 077
        printf 'AGSTUDIO_BASIC_AUTH=:%s\n' "$(openssl rand -hex 16)" > ${envFile}.new
        mv ${envFile}.new ${envFile}
        systemctl restart geosolver.service
      fi
      pw=$(sed -n 's/^AGSTUDIO_BASIC_AUTH=:\(.*\)$/\1/p' ${envFile})
      echo "link:     ${url}"
      echo "password: $pw"
    '';
  };

  geosolver-claude-token = pkgs.writeShellApplication {
    name = "geosolver-claude-token";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.systemd
    ];
    text = ''
      if [ "$(id -u)" -ne 0 ]; then
        echo "run as root: sudo geosolver-claude-token   (paste the token from 'claude setup-token')" >&2
        exit 1
      fi
      if [ -t 0 ]; then
        read -rsp "Paste the token from 'claude setup-token' (input hidden): " tok
        echo
      else
        read -r tok
      fi
      case "$tok" in
        sk-ant-oat*) ;;
        *) echo "that does not look like a 'claude setup-token' token (sk-ant-oat...); nothing changed" >&2; exit 1 ;;
      esac
      umask 077
      install -d -m 0700 ${secretsDir}
      printf 'CLAUDE_CODE_OAUTH_TOKEN=%s\n' "$tok" > ${claudeEnv}.new
      mv ${claudeEnv}.new ${claudeEnv}
      systemctl restart geosolver.service
      if systemctl start geosolver-ai-selftest.service; then
        echo "OK - Describe, Photo and AI explanations are live."
      else
        echo "self-test FAILED - see: journalctl -u geosolver-ai-selftest -n 30" >&2
        exit 1
      fi
    '';
  };

  geosolver-backup = pkgs.writeShellApplication {
    name = "geosolver-backup";
    runtimeInputs = [ pkgs.coreutils ];
    text = ''
      if [ "$(id -u)" -ne 0 ]; then
        echo "run as root: sudo geosolver-backup [dest.db]" >&2
        exit 1
      fi
      dest="''${1:-${cfg.backupDir}/agstudio-$(date +%Y%m%d-%H%M%S).db}"
      install -d -m 0700 "$(dirname "$dest")"
      ${lib.getExe cfg.package} backup "$dest" --db ${stateDir}/agstudio.db
    '';
  };
in
{
  options.services.geosolver = {
    enable = mkEnableOption "GeoSolver, the olympiad geometry prover web app";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.geosolver;
      defaultText = lib.literalExpression "geosolver.packages.\${system}.geosolver";
      description = "The package providing `agstudio`.";
    };

    address = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Address the app listens on. Keep it on loopback and put a TLS proxy or Tailscale Funnel in front.";
    };

    port = mkOption {
      type = types.port;
      default = 8787;
      description = "Port the app listens on.";
    };

    publicHost = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "geo.example.com";
      description = "Hostname the app is served as (AGSTUDIO_PUBLIC_HOST). Required behind a proxy or Funnel, or the Host check refuses every request.";
    };

    behindProxy = mkOption {
      type = types.bool;
      default = true;
      description = "A TLS proxy (nginx, Caddy, Tailscale Funnel) is the only way in: sets AGSTUDIO_TRUST_PROXY=1 and AGSTUDIO_SECURE_COOKIES=1. Only safe with a loopback address.";
    };

    guestMode = mkOption {
      type = types.bool;
      default = true;
      description = "Anyone with the shared password may solve without an account.";
    };

    maxConcurrent = mkOption {
      type = types.nullOr types.ints.positive;
      default = null;
      description = "Simultaneous solves (AGSTUDIO_MAX_CONCURRENT); null = min(max(CPUs, 2), 8). Size memoryMax as slots x 2 GB.";
    };

    cpuQuota = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "400%";
      description = "systemd CPUQuota of the service.";
    };

    memoryMax = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "6G";
      description = "systemd MemoryMax of the service.";
    };

    environment = mkOption {
      type = types.attrsOf types.str;
      default = { };
      example = {
        AGSTUDIO_GUEST_AI_PER_DAY = "60";
      };
      description = "Further AGSTUDIO_* variables (deploy/DEPLOY.md lists them).";
    };

    environmentFiles = mkOption {
      type = types.listOf types.path;
      default = [ ];
      description = "Further environment files, read by systemd as root (keep them mode 0600).";
    };

    ai.package = mkOption {
      type = types.nullOr types.package;
      default = null;
      example = lib.literalExpression "pkgs.claude-code";
      description = "The Claude Code CLI for Describe, Photo and AI explanations (unfree; needs allowUnfree). null turns those features off. Install a subscription token with `sudo geosolver-claude-token`.";
    };

    funnel = {
      enable = mkEnableOption "publishing the app on the internet with Tailscale Funnel (needs services.tailscale and Funnel allowed in the tailnet policy)";
      port = mkOption {
        type = types.enum [
          443
          8443
          10000
        ];
        default = 10000;
        description = "Funnel port. Funnel is per port: everything served on it is public.";
      };
    };

    healthCheck = mkOption {
      type = types.bool;
      default = true;
      description = "Restart the service when /healthz stops answering (checked every minute).";
    };

    backupDir = mkOption {
      type = types.str;
      default = "/var/backup/geosolver";
      description = "Where `geosolver-backup` and the daily backup timer write.";
    };

    dailyBackup = mkOption {
      type = types.bool;
      default = true;
      description = "Write a consistent copy of the database to backupDir every day, keeping 14.";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = !cfg.funnel.enable || cfg.publicHost != null;
        message = "services.geosolver.funnel needs publicHost (the machine's *.ts.net name).";
      }
      {
        assertion = !cfg.behindProxy || lib.hasPrefix "127." cfg.address || cfg.address == "::1";
        message = "services.geosolver.behindProxy trusts X-Forwarded-For; keep address on loopback.";
      }
    ];

    environment.systemPackages = [
      cfg.package
      geosolver-password
      geosolver-backup
    ]
    ++ optional aiOn geosolver-claude-token;

    systemd.services.geosolver-secrets = {
      description = "Generate the GeoSolver shared password";
      wantedBy = [ "multi-user.target" ];
      before = [ "geosolver.service" ];
      requiredBy = [ "geosolver.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        UMask = "0077";
      };
      script = ''
        set -eu
        install -d -m 0700 -o root -g root ${secretsDir}
        if [ ! -s ${envFile} ]; then
          printf 'AGSTUDIO_BASIC_AUTH=:%s\n' "$(${lib.getExe pkgs.openssl} rand -hex 16)" > ${envFile}
        fi
        chmod 0600 ${envFile}
      '';
    };

    systemd.services.geosolver = {
      description = "GeoSolver web app";
      after = [
        "network.target"
        "geosolver-secrets.service"
      ];
      wantedBy = [ "multi-user.target" ];
      environment = {
        AGSTUDIO_BIND = "${cfg.address}:${toString cfg.port}";
        AGSTUDIO_DB = "${stateDir}/agstudio.db";
        AGSTUDIO_GUEST_MODE = if cfg.guestMode then "1" else "0";
      }
      // optionalAttrs (cfg.publicHost != null) {
        AGSTUDIO_PUBLIC_HOST =
          if cfg.funnel.enable then "${cfg.publicHost}:${toString cfg.funnel.port}" else cfg.publicHost;
      }
      // optionalAttrs cfg.behindProxy {
        AGSTUDIO_TRUST_PROXY = "1";
        AGSTUDIO_SECURE_COOKIES = "1";
      }
      // optionalAttrs (cfg.maxConcurrent != null) {
        AGSTUDIO_MAX_CONCURRENT = toString cfg.maxConcurrent;
      }
      // optionalAttrs (!aiOn) { AGSTUDIO_DISABLE_TRANSLATE = "1"; }
      // aiEnv
      // cfg.environment;
      serviceConfig =
        sandbox
        // {
          ExecStart = "${lib.getExe cfg.package} serve";
          Restart = "on-failure";
          RestartSec = 3;
          Nice = 10;
        }
        // optionalAttrs (cfg.cpuQuota != null) { CPUQuota = cfg.cpuQuota; }
        // optionalAttrs (cfg.memoryMax != null) { MemoryMax = cfg.memoryMax; };
    };

    systemd.services.geosolver-ai-selftest = mkIf aiOn {
      description = "GeoSolver: check the Claude CLI works inside the server sandbox";
      environment = aiEnv;
      serviceConfig = sandbox // {
        Type = "oneshot";
        TimeoutStartSec = 120;
      };
      script = ''
        set -u
        ${claudeBin} --version
        if [ -z "''${CLAUDE_CODE_OAUTH_TOKEN:-}" ]; then
          echo "no token installed (sudo geosolver-claude-token)"
          exit 1
        fi
        out=$(cd "$(mktemp -d)" && ${claudeBin} -p "Reply with exactly: GEOSOLVER-OK" --setting-sources "" --strict-mcp-config --mcp-config '{"mcpServers":{}}' --no-session-persistence --disable-slash-commands --permission-mode dontAsk --tools "" 2>&1 </dev/null)
        echo "$out"
        case "$out" in *GEOSOLVER-OK*) exit 0 ;; *) exit 1 ;; esac
      '';
    };

    systemd.services.geosolver-health = mkIf cfg.healthCheck {
      description = "Restart GeoSolver if /healthz stops answering";
      after = [ "geosolver.service" ];
      serviceConfig = {
        Type = "oneshot";
        TimeoutStartSec = 30;
      };
      script = ''
        probe() { ${lib.getExe pkgs.curl} -fsS --max-time 5 -o /dev/null http://${cfg.address}:${toString cfg.port}/healthz; }
        if ! probe; then
          sleep 5
          if ! probe; then
            echo "healthz failed twice; restarting geosolver.service"
            ${pkgs.systemd}/bin/systemctl restart geosolver.service
          fi
        fi
      '';
    };
    systemd.timers.geosolver-health = mkIf cfg.healthCheck {
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnBootSec = "2min";
        OnUnitActiveSec = "1min";
      };
    };

    systemd.services.geosolver-backup = mkIf cfg.dailyBackup {
      description = "Back up the GeoSolver database";
      after = [ "geosolver.service" ];
      serviceConfig = {
        Type = "oneshot";
        UMask = "0077";
      };
      script = ''
        set -eu
        ${lib.getExe geosolver-backup}
        ls -1t ${cfg.backupDir}/agstudio-*.db | tail -n +15 | xargs -r rm -f
      '';
    };
    systemd.timers.geosolver-backup = mkIf cfg.dailyBackup {
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = "daily";
        Persistent = true;
        RandomizedDelaySec = "1h";
      };
    };

    systemd.services.geosolver-funnel = mkIf cfg.funnel.enable {
      description = "Publish GeoSolver with Tailscale Funnel :${toString cfg.funnel.port}";
      after = [
        "tailscaled.service"
        "geosolver.service"
      ];
      requires = [ "tailscaled.service" ];
      wants = [ "geosolver.service" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${lib.getExe config.services.tailscale.package} funnel --bg --https=${toString cfg.funnel.port} http://${cfg.address}:${toString cfg.port}";
        ExecStop = [
          "-${lib.getExe config.services.tailscale.package} funnel --https=${toString cfg.funnel.port} off"
        ];
        Restart = "on-failure";
        RestartSec = 3;
      };
    };
  };
}
