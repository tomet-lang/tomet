{ buildNpmPackage }:
let
  root = ../..;

  # Matches `package.json`'s "publisher"/"name" fields. home-manager's
  # `programs.vscode.extensions` (via nixpkgs' vscode-utils.nix,
  # `toExtensionJsonEntry`) reads these three attributes directly off the
  # extension derivation -- not just any output shape will do, it
  # specifically expects `$out/share/vscode/extensions/${vscodeExtUniqueId}`
  # to hold the extension's files.
  vscodeExtPublisher = "bardmoon";
  vscodeExtName = "typedmark-vscode";
  vscodeExtUniqueId = "${vscodeExtPublisher}.${vscodeExtName}";
in
buildNpmPackage {
  pname = "typedmark-vscode";
  version = "0.1.0";
  src = "${root}/editors/vscode";

  npmDepsHash = "sha256-JsXSQc8TeRokIUCy0BJpISQnkUUPXrBY3ZK3Aq08Kyw=";
  npmBuildScript = "compile";

  passthru = { inherit vscodeExtPublisher vscodeExtName vscodeExtUniqueId; };

  installPhase = ''
    runHook preInstall

    dir=$out/share/vscode/extensions/${vscodeExtUniqueId}
    mkdir -p "$dir"
    cp -r package.json language-configuration.json syntaxes dist "$dir/"

    runHook postInstall
  '';
}
