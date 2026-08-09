{ buildNpmPackage }:
let
  root = ../..;
in buildNpmPackage {
  pname = "typedmark-vscode";
  version = "0.1.0";
  src = "${root}/apps/vscode-extension";

  # Vendors `package-lock.json`'s deps into a fixed-output derivation (npm
  # install happens there, with network access the sandbox otherwise
  # denies); the actual build below runs offline against that. Update
  # this whenever `apps/vscode-extension/package-lock.json` changes --
  # `nix build` reports the correct hash on a mismatch.
  npmDepsHash = "sha256-JsXSQc8TeRokIUCy0BJpISQnkUUPXrBY3ZK3Aq08Kyw=";

  npmBuildScript = "compile";

  # `buildNpmPackage`'s default installPhase assumes an npm library/CLI
  # layout ($out/lib/node_modules/<name> + bin symlinks) -- a VS Code
  # extension instead needs `$out` to *be* the extension directory
  # (package.json/dist/syntaxes/language-configuration.json at the top
  # level), which is what `programs.vscode.extensions` (home-manager)
  # expects.
  installPhase = ''
    mkdir -p $out
    cp -r package.json language-configuration.json syntaxes dist $out/
  '';
}
