{
  rustPlatform,
  # pkg-config,
  # makeWrapper
}:
let
  src = ../..;
in
rustPlatform.buildRustPackage rec {
  inherit src;
  pname = "typedmark";
  version = "0.1.0";

  cargoLock.lockFile = "${src}/Cargo.lock";
  cargoBuildFlags = [
    "-p"
    "typedmark"
  ];
  cargoTestFlags = cargoBuildFlags;

  nativeBuildInputs = [
    # pkg-config
    # makeWrapper
  ];

  buildInputs = [
  ];
}
