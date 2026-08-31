{
  craneLib,
  buildNpmPackage,
}:
let
  src = ../..;

  frontend = buildNpmPackage {
    pname = "tomet-web-frontend";
    version = "0.1.0";
    src = "${src}/apps/web/frontend";

    postUnpack = ''
      mkdir -p $sourceRoot/packages-svelte
      cp -r ${src}/packages/svelte/src $sourceRoot/packages-svelte/
      mkdir -p $sourceRoot/bindings-js
      cp -r ${src}/bindings/js/pkg/* $sourceRoot/bindings-js/
    '';

    npmDepsHash = "sha256-KBtAEZq6nmbL5HH/Ljkx/SoWT2YFS/2ae8Qb0hhoKZs=";
    npmBuildScript = "build";

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp dist/app.js $out/
      cp dist/style.css $out/
      cp dist/index.html $out/
      cp dist/tomet_js_bg.wasm $out/
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
      cp ${frontend}/style.css apps/web/static/style.css
      cp ${frontend}/index.html apps/web/static/index.html
      cp ${frontend}/tomet_js_bg.wasm apps/web/static/tomet_js_bg.wasm
    '';

    doCheck = true;
  }
)
