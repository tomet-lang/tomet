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
        default = tomet;
        tomet = pkgs.callPackage ./pkgs/tomet.nix { inherit craneLib; };
        tomet-lsp = pkgs.callPackage ./pkgs/tomet-lsp.nix { inherit craneLib; };
        tomet-web = pkgs.callPackage ./pkgs/tomet-web.nix { inherit craneLib; };
        vscodeExtension = pkgs.callPackage ./pkgs/vscode-extension.nix { };
      };

      devShells.default = pkgs.callPackage ./dev.nix {
        inherit inputs craneLib;
        tomet = pkgs.callPackage ./pkgs/tomet.nix { inherit craneLib; };
        tomet-lsp = pkgs.callPackage ./pkgs/tomet-lsp.nix { inherit craneLib; };
        tomet-web = pkgs.callPackage ./pkgs/tomet-web.nix { inherit craneLib; };
      };

      treefmt = import ./formatter.nix;
    };
}
