{
  pkgs,
  mkShell,
  fenix,

  tomet,
  tomet-lsp,
  tomet-web,
  twrit,
  tmtbook,
  ...
}:
let
  rust-toolchain = fenix.combine [
    (fenix.stable.withComponents [
      "cargo"
      "clippy"
      "rustc"
      "rust-src"
      # "rust-analyzer"
    ])
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
    fenix.targets.wasm32-wasip2.stable.rust-std
  ];
in
mkShell rec {
  buildInputs = with pkgs; [
    tomet
    tomet-lsp
    tomet-web
    twrit
    tmtbook
    pagefind

    #[ CMake ]
    cmake
    ninja

    #[ Rust ]
    rust-toolchain
    cargo-edit
    cargo-outdated
    cargo-nextest
    wasm-bindgen-cli

    #[ Pandoc ]
    haskellPackages.pandoc-cli
    #[ VS Code extension (apps/vscode-extension) ]
    nodejs
    pnpm
    #[ Python bindings (bindings/python) ]
    python3

    #[ Misc ]
    pkg-config
  ];

  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
  ];

  shellHook = ''
    alias nvim='nvim --cmd "set rtp+=$PWD/editors/neovim"'

    echo "🦀 Rust Nvim"
  '';
}
