{
  inputs,
  pkgs,
  stdenv,
  mkShell,

  typedmark,
  typedmark-lsp,
  ...
}:
let
  fenix = inputs.fenix.packages.${stdenv.hostPlatform.system};
  rust-toolchain = fenix.combine [
    (fenix.stable.withComponents [
      "cargo"
      "clippy"
      "rustc"
      "rust-src"
      # "rust-analyzer"
    ])
    fenix.targets.wasm32-wasip2.stable.rust-std
  ];
in
mkShell rec {
  buildInputs = with pkgs; [
    typedmark
    typedmark-lsp
    # typedmark-web

    #[ CMake ]
    cmake
    ninja

    #[ Rust ]
    rust-toolchain
    cargo-edit
    cargo-outdated
    cargo-nextest

    #[ VS Code extension (apps/vscode-extension) ]
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
