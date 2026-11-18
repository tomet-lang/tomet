{
  description = "Tomet";

  nixConfig = {
    extra-substituters = [ "https://tomet.cachix.org" ];
    extra-trusted-public-keys = [ "tomet.cachix.org-1:9c/iO8Tb6YOM+3r55t12W3fOJK+66itPEaIe8Rs9MDw=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # --- Rust ---
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };

    # --- Tools ---
    twrit = {
      url = "github:tomet-lang/twrit";
      inputs.tomet.follows = "";
      inputs.tomet-shelf.follows = "";
      inputs.catasto.follows = "";
      inputs.sisho.follows = "";
    };
    tomet-shelf = {
      url = "github:tomet-lang/tomet-shelf";
      inputs.tomet.follows = "";
      inputs.twrit.follows = "";
      inputs.catasto.follows = "";
      inputs.sisho.follows = "";
    };
    catasto = {
      url = "github:tomet-lang/fs-catasto";
      inputs.tomet.follows = "";
      inputs.twrit.follows = "";
      inputs.tomet-shelf.follows = "";
      inputs.sisho.follows = "";
    };
    sisho = {
      url = "github:tomet-lang/sisho";
      inputs.tomet.follows = "";
      inputs.twrit.follows = "";
      inputs.tomet-shelf.follows = "";
      inputs.sisho-desk.follows = "";
      inputs.catasto.follows = "";
    };
  };

  outputs = inputs: import ./nix inputs;
}
