{
  craneLib,
  buildNpmPackage,
}:
let
  src = ../..;

  # Builds the CodeMirror-based editor bundle (`apps/web/frontend`) the
  # same way `vscode-extension.nix` builds the VS Code extension's own
  # TypeScript -- a `buildNpmPackage` derivation with a pinned
  # `npmDepsHash`, so the network-dependent `npm install` step stays a
  # separate, cacheable fixed-output derivation instead of living inside
  # the (otherwise pure) Rust build below.
  frontend = buildNpmPackage {
    pname = "tomet-web-frontend";
    version = "0.1.0";
    src = "${src}/apps/web/frontend";

    npmDepsHash = "sha256-6P6NZgSIBgAT9nZuMGVyw5K6vtao/DmOjTtqyohYD2Q=";
    npmBuildScript = "build";

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp dist/app.js $out/
      runHook postInstall
    '';
  };

  commonArgs = {
    inherit src;

    pname = "tomet-web";
    version = "0.1.0";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    cargoExtraArgs = "-p tomet-web";

    # `frontend`'s own `$out` is a separate store path; copy its built
    # bundle into place before `include_str!("../static/app.js")`
    # compiles it in.
    preBuild = ''
      cp ${frontend}/app.js apps/web/static/app.js
    '';

    doCheck = true;
  }
)
