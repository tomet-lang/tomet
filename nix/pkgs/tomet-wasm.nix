# `@tomet/wasm` as an npm package directory: bindings/js compiled to wasm32 and
# run through wasm-bindgen, next to the hand-written index.js / index.d.ts.
#
# Consumers (packages/astro, the website) take this instead of a locally built
# bindings/js/pkg, so the renderer they ship is the revision they lock.
{
  lib,
  craneLib,
  fenix,
  stdenvNoCC,
  wasm-bindgen-cli_0_2_127,
}:
let
  # The CLI must equal the `wasm-bindgen` crate pinned in Cargo.toml (=0.2.127);
  # wasm-bindgen refuses to process a module built by another version.
  wasm-bindgen-cli = wasm-bindgen-cli_0_2_127;

  npmPackage = lib.importJSON ../../bindings/js/package.json;

  toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
  ];
  craneWasm = craneLib.overrideToolchain toolchain;

  commonArgs = {
    src = ../..;
    pname = "tomet-js";
    inherit (npmPackage) version;

    cargoExtraArgs = "-p tomet-js";
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    # Tests target the host; there is nothing to run on wasm32 here.
    doCheck = false;
  };

  cargoArtifacts = craneWasm.buildDepsOnly commonArgs;

  wasmModule = craneWasm.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      # crane's default install looks for executables; a cdylib is a .wasm file.
      installPhaseCommand = ''
        install -Dm644 target/wasm32-unknown-unknown/release/tomet_js.wasm $out/lib/tomet_js.wasm
      '';
    }
  );
in
stdenvNoCC.mkDerivation {
  pname = "tomet-wasm";
  inherit (npmPackage) version;

  src = lib.fileset.toSource {
    root = ../../bindings/js;
    fileset = lib.fileset.unions [
      ../../bindings/js/package.json
      ../../bindings/js/index.js
      ../../bindings/js/index.d.ts
    ];
  };

  nativeBuildInputs = [ wasm-bindgen-cli ];

  buildPhase = ''
    runHook preBuild
    # `--target web`: index.js hands the bytes to the default init itself.
    wasm-bindgen --target web --out-dir pkg ${wasmModule}/lib/tomet_js.wasm
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    cp -r . $out
    runHook postInstall
  '';

  passthru = { inherit wasmModule; };
}
