{
  craneLib,
}:
let
  src = ../..;

  commonArgs = {
    inherit src;

    pname = "typedmark-web";
    version = "0.1.0";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    cargoExtraArgs = "-p typedmark-web";

    doCheck = true;
  }
)
