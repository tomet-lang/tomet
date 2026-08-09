{
  mkShell,
  pkgs,
  ...
}:
mkShell rec {
  buildInputs = with pkgs; [
    (pkgs.callPackage ./pkgs/typedmark.nix { })

    #[ C++ ]
    cmake
    ninja

    #[ Rust ]
    (rust-bin.stable.latest.default.override {
      extensions = [
        "clippy"
        "rust-src"
      ];
      # wasm32-wasip2: needed to build Zed extensions locally (e.g.
      # apps/zed-extension in the typedmark repo) -- Zed shells out to
      # `rustc`/`cargo` on $PATH to compile them and doesn't manage its own
      # toolchain or targets.
      targets = [ "wasm32-wasip2" ];
    })
    cargo
    cargo-edit
    cargo-outdated
    rustc
    cargo-nextest

    #[ VS Code extension (apps/vscode-extension) ]
    bun
    nodejs

    #[ Misc ]
    pkg-config
  ];

  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
  ];

  shellHook = ''
    echo "🦀 Rust"
  '';
}
