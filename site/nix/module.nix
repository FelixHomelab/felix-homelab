{ self }:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.felix-homelab-site;

  inherit (lib)
    mkIf mkOption mkEnableOption mkMerge types literalExpression optional;

  # 服务端二进制与它用到的静态资源、内容都在 store 里（只读），
  # 会变的只有数据库与上传的图片。
  package = cfg.package;
  siteRoot = "${package}/share/felix-homelab-site/site";
  contentDir = "${package}/share/felix-homelab-site/content";

  # 只有真的经 HTTPS 提供服务时才给 cookie 加 Secure：加了却走明文会让浏览器
  # 直接不回传 cookie，表现成「登录后一刷新就掉登录」。
  behindTls = cfg.nginx.enable && (cfg.nginx.forceSSL || cfg.nginx.enableACME);
in {
  options.services.felix-homelab-site = {
    enable = mkEnableOption "Felix Homelab 社区站";

    package = mkOption {
      type = types.package;
      # 注意按**目标系统**的 system 取，不是求值 flake 时的系统
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = literalExpression "self.packages.\${system}.default";
      description = "要运行的服务端包（含二进制、前端产物与内容）";
    };

    siteUrl = mkOption {
      type = types.str;
      example = "https://example.com";
      description = ''
        站点对外的根地址，末尾不带斜杠。

        **上线必须填写**：RSS 与 sitemap 都要求绝对 URL，不填会退回本机地址，
        产出里的链接就全是错的，而本机测试时看不出来。
      '';
    };

    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "监听地址。默认只绑回环，由反向代理对外。";
    };

    port = mkOption {
      type = types.port;
      default = 8080;
      description = "监听端口。";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/felix-homelab-site";
      description = "SQLite 数据库与上传图片的存放目录。**备份要连它一起备。**";
    };

    user = mkOption {
      type = types.str;
      default = "felix-homelab";
      description = "运行服务的系统用户。";
    };

    group = mkOption {
      type = types.str;
      default = "felix-homelab";
      description = "运行服务的系统组。";
    };

    environmentFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      example = "/run/secrets/felix-homelab-site.env";
      description = ''
        秘密环境变量文件，首次启动至少要给出 `ADMIN_PASSWORD=`（配合
        `ADMIN_USERNAME=` 创建站长账号）。

        **绝不要把秘密写进 `environment`**：Nix store 全库可读，任何进了 store 的
        值对所有本地用户可见，而且会永久留在系统世代里、删都删不干净。
        用 sops-nix 或 agenix 生成这个文件，或者手工放在 /run 下（tmpfs，重启即失）。
      '';
    };

    nginx = {
      enable = mkEnableOption "为本站配置 nginx 反向代理";

      domain = mkOption {
        type = types.str;
        default = "";
        example = "example.com";
        description = "对外域名；nginx 的 server_name 与证书都用它。";
      };

      enableACME = mkOption {
        type = types.bool;
        default = true;
        description = "用 ACME 自动申请证书。需要域名已解析到本机且 80 端口可达。";
      };

      forceSSL = mkOption {
        type = types.bool;
        default = true;
        description = "把 HTTP 跳转到 HTTPS。";
      };
    };

    backup = {
      enable = mkEnableOption "定时备份数据库与上传目录";

      interval = mkOption {
        type = types.str;
        default = "daily";
        description = "备份频率，systemd 定时器语法（如 daily、*-*-* 04:00:00）。";
      };

      destination = mkOption {
        type = types.path;
        default = "/var/backup/felix-homelab-site";
        description = "备份文件存放目录。异地同步请另行配置。";
      };

      keep = mkOption {
        type = types.int;
        default = 14;
        description = "保留最近多少份备份，更早的自动删除。";
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    {
      users.groups.${cfg.group} = { };

      users.users.${cfg.user} = {
        isSystemUser = true;
        group = cfg.group;
        home = cfg.dataDir;
        description = "Felix Homelab 社区站";
      };

      # 用 tmpfiles 而不是 StateDirectory：dataDir 是可配置的，tmpfiles 能处理任意路径
      systemd.tmpfiles.rules = [
        "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} -"
        "d ${cfg.dataDir}/uploads 0750 ${cfg.user} ${cfg.group} -"
      ];

      systemd.services.felix-homelab-site = {
        description = "Felix Homelab 社区站（Leptos + Axum + SQLite）";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        wants = [ "network.target" ];

        environment = {
          # 静态资源与内容都在 store 里；跑起来必须告诉 Leptos 它们在哪，
          # 否则二进制会去相对路径找，找不到就等于站点没有样式与脚本。
          LEPTOS_OUTPUT_NAME = "felix-homelab-site";
          LEPTOS_SITE_ROOT = siteRoot;
          LEPTOS_SITE_PKG_DIR = "pkg";
          LEPTOS_SITE_ADDR = "${cfg.host}:${toString cfg.port}";
          LEPTOS_ENV = "PROD";

          DATABASE_URL = "sqlite://${cfg.dataDir}/site.db";
          UPLOAD_DIR = "${cfg.dataDir}/uploads";
          CONTENT_DIR = contentDir;

          SITE_URL = cfg.siteUrl;
          RUST_LOG = "info,sqlx=warn";
        } // lib.optionalAttrs behindTls {
          COOKIE_SECURE = "1";
        };

        serviceConfig = {
          ExecStart = "${package}/bin/felix-homelab-site";
          User = cfg.user;
          Group = cfg.group;
          WorkingDirectory = cfg.dataDir;
          Restart = "always";
          RestartSec = "5";

          # 权限收拢：只让服务写自己的数据目录，其余文件系统一律只读。
          # SQLite 与上传目录是它唯一需要写的东西。
          ReadWritePaths = [ cfg.dataDir ];
          ProtectSystem = "strict";
          ProtectHome = true;
          PrivateTmp = true;
          NoNewPrivileges = true;
          PrivateDevices = true;
          ProtectKernelTunables = true;
          ProtectControlGroups = true;
          RestrictSUIDSGID = true;
        } // lib.optionalAttrs (cfg.environmentFile != null) {
          EnvironmentFile = cfg.environmentFile;
        };
      };
    }

    (mkIf cfg.nginx.enable {
      services.nginx = {
        enable = true;
        recommendedProxySettings = true;
        recommendedGzipSettings = true;

        virtualHosts.${cfg.nginx.domain} = {
          inherit (cfg.nginx) enableACME forceSSL;

          locations."/" = {
            proxyPass = "http://${cfg.host}:${toString cfg.port}";
            # 静态资源由应用自己带 immutable 头提供，这里不要再叠一层缓存策略
            extraConfig = ''
              proxy_set_header X-Forwarded-Proto $scheme;
            '';
          };
        };
      };

      networking.firewall.allowedTCPPorts = [ 80 443 ];
    })

    (mkIf cfg.backup.enable {
      systemd.services.felix-homelab-site-backup = {
        description = "备份 Felix Homelab 社区站的数据";
        # 服务在跑（WAL 模式下文件随时在变），但备份用它自己的 sqlite3 快照，
        # 不需要停服
        serviceConfig = {
          Type = "oneshot";
          User = cfg.user;
          Group = cfg.group;
          # 目标目录由上面的 tmpfiles 规则创建并授权，这里不再另设 StateDirectory
          WorkingDirectory = cfg.backup.destination;
        };

        script = ''
          set -euo pipefail
          mkdir -p ${cfg.backup.destination}
          stamp=$(date +%Y%m%d-%H%M%S)
          out=${cfg.backup.destination}/backup-$stamp

          mkdir -p "$out"

          # 用 SQLite 自己的 .backup 而不是直接拷文件：数据库跑在 WAL 模式下，
          # 直接 cp 很可能拿到一个缺事务的库。
          ${pkgs.sqlite}/bin/sqlite3 ${cfg.dataDir}/site.db ".backup '$out/site.db'"

          if [ -d ${cfg.dataDir}/uploads ]; then
            cp -r ${cfg.dataDir}/uploads "$out/uploads"
          fi

          tar -C ${cfg.backup.destination} -czf "$out.tar.gz" "backup-$stamp"
          rm -rf "$out"

          # 只留最近 keep 份
          ls -1t ${cfg.backup.destination}/backup-*.tar.gz 2>/dev/null \
            | tail -n +$(( ${toString cfg.backup.keep} + 1 )) \
            | xargs -r rm -f
        '';
      };

      systemd.timers.felix-homelab-site-backup = {
        description = "定时备份 Felix Homelab 社区站的数据";
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnCalendar = cfg.backup.interval;
          Persistent = true;
        };
      };

      systemd.tmpfiles.rules = [
        "d ${cfg.backup.destination} 0750 ${cfg.user} ${cfg.group} -"
      ];
    })
  ]);
}
