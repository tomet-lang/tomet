{
  mkShell,
  lib,
  pkgs,
  ...
}:
mkShell rec {
  buildInputs = with pkgs; [
    pkg-config
    cmake
    ninja

    #[ Rust ]
    (rust-bin.stable.latest.default.override {
      extensions = [
        "clippy"
        "rust-src"
      ];
    })
    cargo
    cargo-edit
    cargo-outdated
    rustc
    cargo-nextest
  ];

  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
  ];

  shellHook = ''
    echo "🦀 Rust"
  '';
}
