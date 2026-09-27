{
  description = "Felix Homelab 社区站 —— 用 Leptos + Axum + SQLite 写的自托管社区站";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Leptos 的前端产物需要 wasm32-unknown-unknown 的 std，而 nixpkgs 的 rustc 不带
    # 这个目标（也没有 pkgsCross.wasm32-unknown-unknown）。rust-overlay 是社区给出
    # 多目标工具链的标准做法。
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      # 与 AWCC 一致：只声明真正用得到的平台，不引 flake-utils
      systems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system:
          f {
            inherit system;
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
          });

      # 工具链只在一处定义，devShell 与 package 共用同一份——否则最容易出现
      # 「开发能编、打包编不过」这类分歧。
      mkToolchain = pkgs: pkgs.rust-bin.stable.latest.default.override {
        targets = [ "wasm32-unknown-unknown" ];
        extensions = [ "rustfmt" "clippy" "rust-analyzer" ];
      };

      mkPackage = pkgs: pkgs.callPackage ./nix/package.nix {
        toolchain = mkToolchain pkgs;
      };
    in {
      devShells = forAllSystems ({ pkgs, ... }: {
        default = pkgs.mkShell {
          name = "felix-homelab-site-dev";

          packages = [
            (mkToolchain pkgs)
            # cargo-leptos 负责「一条命令同时编出 wasm 与服务端二进制」
            pkgs.cargo-leptos
            # 版本必须与 Cargo.lock 里的 wasm-bindgen 一致：cargo-leptos 会检查，
            # 不一致时它会联网自己下载，在纯 Nix 构建里会直接失败。
            pkgs.wasm-bindgen-cli
            # release 构建要用 wasm-opt 压缩 wasm 包
            pkgs.binaryen
            # 本地跑服务与手工查库都要用
            pkgs.sqlite
            pkgs.curl
          ];

          shellHook = ''
            echo "Felix Homelab 社区站 开发环境"
            echo "  rustc  $(rustc --version)"
            echo "  wasm   $(rustc --print target-libdir --target wasm32-unknown-unknown 2>/dev/null || echo '目标缺失')"
            echo
            echo "常用命令："
            echo "  cargo leptos watch     # 开发，改动自动重编"
            echo "  cargo leptos serve     # 起服务，http://127.0.0.1:8080"
            echo "  cargo leptos build --release"
          '';
        };
      });

      packages = forAllSystems ({ pkgs, ... }: {
        default = mkPackage pkgs;
        felix-homelab-site = mkPackage pkgs;
      });

      # 模块拿到的 `pkgs` 是**目标系统**的，因此默认包必须按 `pkgs.system` 取，
      # 不能按求值 flake 时的系统取——两者在交叉或远程部署时并不相同。
      nixosModules = {
        felix-homelab-site = import ./nix/module.nix { inherit self; };
        default = self.nixosModules.felix-homelab-site;
      };

      # `nix flake check` 会真的去构建它；只想快速看求值通不通就加 --no-build
      checks = forAllSystems ({ system, ... }: {
        package = self.packages.${system}.default;
      });
    };
}
