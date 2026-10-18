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
      vscodeExtension = pkgs.callPackage ./pkgs/vscode-extension.nix { };
    in
    {
      packages = {
        default = tomet;
        inherit
          tomet
          tomet-lsp
          vscodeExtension
          ;
      };

      devShells.default = pkgs.callPackage ./dev.nix {
        inherit
          inputs
          craneLib
          tomet
          tomet-lsp
          ;
        fenix = inputs.fenix.packages.${pkgs.stdenv.hostPlatform.system};
        # The other tools; tomet itself is the one built from this checkout.
        tools =
          let
            system = pkgs.stdenv.hostPlatform.system;
            sisho = inputs.sisho.packages.${system};
          in
          inputs.sisho.legacyPackages.${system}.combine [
            sisho.sisho
            sisho.twrit
            sisho.tomet-shelf
          ];
      };

      treefmt = import ./formatter.nix {
        inherit tomet;
      };
    };
}
