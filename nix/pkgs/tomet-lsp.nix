{
  craneLib,
}:
let
  src = ../..;

  commonArgs = {
    inherit src;

    pname = "tomet-lsp";
    version = "0.2.0";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    cargoExtraArgs = "-p tomet-lsp";

    doCheck = true;
  }
)
