{
  flake-parts,
  ...
}@inputs:
flake-parts.lib.mkFlake { inherit inputs; } {
  systems = [
    "x86_64-linux"
    "aarch64-linux"
    "aarch64-darwin"
  ];
  imports = [
    inputs.treefmt-nix.flakeModule
  ];

  perSystem =
    { pkgs, ... }:
    let
      craneLib = inputs.crane.mkLib pkgs;
    in
    {
      packages = rec {
        default = typedmark;
        typedmark = pkgs.callPackage ./pkgs/typedmark.nix { inherit craneLib; };
        typedmark-lsp = pkgs.callPackage ./pkgs/typedmark-lsp.nix { inherit craneLib; };
        typedmark-web = pkgs.callPackage ./pkgs/typedmark-web.nix { inherit craneLib; };
        vscodeExtension = pkgs.callPackage ./pkgs/vscode-extension.nix { };
      };

      devShells.default = pkgs.callPackage ./dev.nix {
        inherit inputs craneLib;
        typedmark = pkgs.callPackage ./pkgs/typedmark.nix { inherit craneLib; };
        typedmark-lsp = pkgs.callPackage ./pkgs/typedmark-lsp.nix { inherit craneLib; };
        typedmark-web = pkgs.callPackage ./pkgs/typedmark-web.nix { inherit craneLib; };
      };

      treefmt = import ./formatter.nix;
    };
}
