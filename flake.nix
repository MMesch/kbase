{
  description = "kg - Knowledge Graph CLI development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          # Core tools
          git
          curl
          jq
          ripgrep
          fd

          # Rust ecosystem
          rustc
          cargo
          clippy
          rustfmt
          rust-analyzer

          # Build dependencies
          clang
          libclang.lib
          openssl
          pkg-config

          # TypeScript/Deno ecosystem
          deno
          nodejs
        ];

        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

        shellHook = ''
          echo "kg development environment ready"
          echo "Use: nix develop"
        '';
      };
    });
}