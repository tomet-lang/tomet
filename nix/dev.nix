{
  lib,
  pkgs,
  mkShell,
  fenix,
  tomet,
  tomet-lsp,
  twrit,
  tmtbook,
  ...
}:
let
  rustToolchain = fenix.combine [
    (fenix.stable.withComponents [
      "cargo"
      "clippy"
      "rustc"
      "rust-src"
      "rustfmt"
      "rust-analyzer"
    ])
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
    fenix.targets.wasm32-wasip2.stable.rust-std
  ];
in
mkShell rec {
  buildInputs = with pkgs; [
    tomet
    tomet-lsp
    twrit
    tmtbook
    pagefind

    #= Develop
    #== Build
    pkg-config
    #== CMake
    cmake
    ninja
    #== Rust
    rustToolchain
    cargo-edit
    cargo-outdated
    cargo-nextest
    wasm-bindgen-cli
    binaryen # wasm-opt: the size a browser downloads is after this
    twiggy # which crates the wasm size is spent in
    #== Tree-sitter @dir(crates/tree-sitter-tomet)
    tree-sitter # regenerates parser.c/grammar.json/node-types.json from grammar.js
    #== Python
    python313
    #== Pandoc
    haskellPackages.pandoc-cli
    #== VS Code extension @dir(apps/vscode-extension)
    nodejs
    pnpm
  ];

  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  LD_LIBRARY_PATH = lib.makeLibraryPath [
    pkgs.python313
  ];

  PYO3_PYTHON = "${pkgs.python313}/bin/python3";
  PYO3_USE_ABI3_FORWARD_COMPATIBILITY = "1";

  shellHook = ''
    alias nvim='nvim --cmd "set rtp+=$PWD/editors/neovim"'

    echo "🧪 Rust Tomet"
  '';
}
