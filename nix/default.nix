{
  nixpkgs,
  ...
}@inputs:
let
  systems = [
    "x86_64-linux"
    "aarch64-linux"
    "aarch64-darwin"
  ];
  forAllSystems =
    f:
    nixpkgs.lib.genAttrs systems (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        craneLib = inputs.crane.mkLib pkgs;
      in
      f {
        inherit system pkgs craneLib;
      }
    );
in
{
  packages = forAllSystems (
    { pkgs, craneLib, ... }: rec {
      default = typedmark;
      typedmark = pkgs.callPackage ./pkgs/typedmark.nix { inherit craneLib; };
      vscodeExtension = pkgs.callPackage ./pkgs/vscode-extension.nix { };
    }
  );

  devShells = forAllSystems (
    { pkgs, craneLib, ... }: {
      default = pkgs.callPackage ./dev.nix { inherit inputs craneLib; };
    }
  );

  formatter = forAllSystems (
    { pkgs, ... }:
    let
      treefmt = import ./formatter.nix {
        inherit pkgs inputs;
      };
    in
    treefmt.wrapper
  );
}
