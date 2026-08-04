{
  nixpkgs,
  rust-overlay,
  ...
}@inputs:
let
  # https://github.com/numtide/go2nix/blob/main/flake.nix
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
          overlays = [ (import rust-overlay) ];
        };
      in
      f system pkgs
    );
in
{
  packages = forAllSystems (
    _: pkgs: rec {
      default = typedmark;
      typedmark = pkgs.callPackage ./pkgs/cettila.nix { };
    }
  );
  devShells = forAllSystems (
    _: pkgs: {
      default = pkgs.callPackage ./dev.nix { };
    }
  );
  formatter = forAllSystems (
    _: pkgs:
    let
      treefmt = import ./formatter.nix { inherit pkgs inputs; };
    in
    treefmt.wrapper
  );
}
