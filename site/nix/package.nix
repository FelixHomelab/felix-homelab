{ lib
, makeRustPlatform
, toolchain
, cargo-leptos
, wasm-bindgen-cli
, binaryen
, pkg-config
}:

let
  # 用 rust-overlay 的工具链搭 rustPlatform：nixpkgs 自带的 rustc 不带
  # wasm32-unknown-unknown 的 std，而 Leptos 的前端产物正需要它。
  rustPlatform = makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "felix-homelab-site";
  version = "0.1.0";

  # 必须排掉这些目录：`target/` 是构建产物（可能几个 G），`data/` 是运行时数据
  # （SQLite 与上传的图片，绝不该进 Nix store —— store 是全库可读且永久留存的）。
  src = lib.cleanSourceWith {
    src = ./..;
    filter = path: _type:
      let base = builtins.baseNameOf (toString path);
      in !(builtins.elem base [
        "target"
        "data"
        "result"
        ".direnv"
        ".git"
      ]);
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    cargo-leptos
    # 版本必须与 Cargo.lock 里的 wasm-bindgen 一致，否则 cargo-leptos 会试图联网
    # 自己下载，在 Nix 构建沙箱里必然失败。两边版本由 `flake.nix` 与 Cargo.lock
    # 共同决定，改一个就要改另一个。
    wasm-bindgen-cli
    # release 构建会调用 wasm-opt 压缩 wasm 包；缺了它 cargo-leptos 会直接报
    # 「wasm-opt is required but was not found」。
    binaryen
    pkg-config
  ];

  # 项目没有测试；`nix flake check` 只验证它能构建出来
  doCheck = false;

  # 不用 buildRustPackage 默认的 `cargo build`：这个项目要靠 cargo-leptos 同时
  # 产出浏览器端 wasm 包与服务端二进制。
  buildPhase = ''
    runHook preBuild
    export LEPTOS_OUTPUT_NAME=felix-homelab-site
    cargo leptos build --release
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin $out/share/felix-homelab-site

    install -m755 target/release/felix-homelab-site $out/bin/

    # 前端产物：wasm 包、JS 胶水与编译后的 CSS
    cp -r target/site $out/share/felix-homelab-site/site

    # 内容（Markdown）在构建时一并固定进 store：内容改动 = 重新构建 + 切换世代，
    # 因此「线上跑的到底是哪一版内容」是可追溯的。
    cp -r content $out/share/felix-homelab-site/content

    runHook postInstall
  '';

  meta = {
    description = "Felix Homelab 社区站（Leptos + Axum + SQLite）";
    homepage = "http://127.0.0.1:3000/Felix/felix-homelab-site";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "felix-homelab-site";
  };
})
