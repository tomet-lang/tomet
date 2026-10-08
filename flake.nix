{
  description = "Tomet - A structured markup language for thought and document graphs";

  nixConfig = {
    extra-substituters = [ "https://tomet.cachix.org" ];
    extra-trusted-public-keys = [
      "tomet.cachix.org-1:9c/iO8Tb6YOM+3r55t12W3fOJK+66itPEaIe8Rs9MDw="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    #= Rust
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };

    #= Tool
    twrit = {
      url = "github:tomet-lang/tomet-writ";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.tomet.follows = "";
    };
    tomet-book = {
      url = "github:tomet-lang/tomet-book";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.tomet.follows = "";
    };
  };

  outputs = inputs: import ./nix inputs;
}
