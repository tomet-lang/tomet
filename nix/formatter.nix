{ pkgs, inputs, ... }:
let
  treefmtModule = inputs.treefmt-nix.lib.evalModule pkgs {
    projectRootFile = "flake.nix";
    programs = {
      #[ Nix ]
      nixfmt.enable = true;
      statix.enable = true;
      deadnix.enable = true;
      #[ Shell ]
      shfmt.enable = true;
      shellcheck.enable = true;

      #[ Main ]
      rustfmt.enable = true; # Rust
      taplo.enable = true; # Toml

      #[ Sub ]
      # prettier.enable = true;
      biome.enable = true;
    };
    settings = {
      global.excludes = [ ]; # https://github.com/numtide/treefmt-nix/issues/171
      biome = {
        includes = [
          "*.js"
          "*.ts"
          "*.jsx"
          "*.tsx"
          "*.json"
        ];
      };

      rustfmt = {
        includes = [ "*.rs" ];
      };

      shfmt = {
        includes = [ "*.sh" ];
      };
    };
  };
in
treefmtModule.config.build
