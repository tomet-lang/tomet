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

    #[ Dev ]
    twrit.url = "git+https://github.com/tomet-lang/tomet-writ.git";
    tmtbook.url = "git+https://github.com/tomet-lang/tomet-book.git";
  };

  outputs = inputs: import ./nix inputs;
}
