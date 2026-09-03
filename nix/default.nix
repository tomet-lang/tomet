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
      tomet = pkgs.callPackage ./pkgs/tomet.nix { inherit craneLib; };
      tomet-lsp = pkgs.callPackage ./pkgs/tomet-lsp.nix { inherit craneLib; };
      tomet-web = pkgs.callPackage ./pkgs/tomet-web.nix { inherit craneLib; };
      vscodeExtension = pkgs.callPackage ./pkgs/vscode-extension.nix { };
    in
    {
      packages = {
        default = tomet;
        inherit
          tomet
          tomet-lsp
          tomet-web
          vscodeExtension
          ;
      };

      devShells.default = pkgs.callPackage ./dev.nix {
        inherit
          inputs
          craneLib
          tomet
          tomet-lsp
          tomet-web
          ;
        twrit = inputs.twrit.packages.${pkgs.system}.twrit;
      };

      treefmt = import ./formatter.nix {
        inherit tomet;
      };
    };
}
