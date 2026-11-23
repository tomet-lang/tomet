# `@tomet/astro` as an npm package directory, compiled from src/ (never the
# checked-in dist/) and carrying the `@tomet/wasm` of the same revision in its
# own node_modules: Node resolves the import from the package's real path, so
# the renderer cannot come from anywhere else.
{
  lib,
  buildNpmPackage,
  jq,
  tomet-wasm,
}:
let
  npmPackage = lib.importJSON ../../packages/astro/package.json;
in
buildNpmPackage {
  pname = "tomet-astro";
  inherit (npmPackage) version;

  # bindings/js is here only because the lockfile links `@tomet/wasm` to it;
  # the link is pointed at the built package before anything uses it.
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../packages/astro/package.json
      ../../packages/astro/package-lock.json
      ../../packages/astro/tsconfig.json
      ../../packages/astro/README.md
      ../../packages/astro/src
      ../../packages/astro/test
      ../../bindings/js/package.json
    ];
  };
  sourceRoot = "source/packages/astro";

  nativeBuildInputs = [ jq ];

  npmDepsHash = "sha256-yxXUt4eCnsYPTEVth498wSOWHO5U0Jj8tPaHwm8X6z8=";

  postConfigure = ''
    rm node_modules/@tomet/wasm
    ln -s ${tomet-wasm} node_modules/@tomet/wasm
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck
    node --experimental-strip-types --test test/index.test.ts
    runHook postCheck
  '';

  # The installed shape of a package: what `npm pack` ships, without the
  # build-time fields. npm installs a `file:` dependency's devDependencies into
  # it as if it were a project under development, and this one is read-only.
  installPhase = ''
    runHook preInstall
    mkdir -p $out/node_modules/@tomet
    jq 'del(.devDependencies, .scripts)' package.json > $out/package.json
    cp -r README.md dist $out/
    ln -s ${tomet-wasm} $out/node_modules/@tomet/wasm
    runHook postInstall
  '';

  passthru = { inherit tomet-wasm; };
}
