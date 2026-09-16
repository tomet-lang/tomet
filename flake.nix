{
  description = "Rust Environment with Fenix and Crane";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix.url = "github:numtide/treefmt-nix";

    #[ Rust ]
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";

    #[ Tool ]
    twrit = {
      url = "github:tomet-lang/tomet-writ";
      inputs.tomet.follows = "";
    };
    tomet-book = {
      url = "github:tomet-lang/tomet-book";
      inputs.tomet.follows = "";
    };
  };

  outputs = inputs: import ./nix inputs;
}
