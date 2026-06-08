{
  description = "kbase - Knowledge Graph CLI for markdown notes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
      craneLib = crane.mkLib pkgs;

      # Common build inputs
      nativeBuildInputs = with pkgs; [
        pkg-config
        clang
        makeWrapper
      ];

      buildInputs = with pkgs; [
        openssl
        libclang.lib
        # RocksDB dependencies
        snappy
        zlib
        lz4
        zstd
        bzip2
        # ONNX Runtime (loaded dynamically at runtime)
        onnxruntime
      ];

      # Common environment
      commonEnv = {
        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
        CXX = "${pkgs.clang}/bin/clang++";
        CC = "${pkgs.clang}/bin/clang";
        RUST_MIN_STACK = "16777216";
      };

      # Filter source - include Rust files plus test fixtures
      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (craneLib.filterCargoSources path type) ||
          (builtins.match ".*/(specs|examples)/.*" path != null);
      };

      # Common arguments for all builds
      commonArgs = {
        inherit src nativeBuildInputs buildInputs;
        strictDeps = true;
      } // commonEnv;

      # Build dependencies only (cached separately)
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      # Build the actual package
      kbase = craneLib.buildPackage (commonArgs // {
        inherit cargoArtifacts;

        postInstall = ''
          wrapProgram $out/bin/kbase \
            --set ORT_DYLIB_PATH ${pkgs.onnxruntime}/lib/libonnxruntime.so
        '';

        meta = with pkgs.lib; {
          description = "Knowledge graph CLI for markdown notes";
          homepage = "https://github.com/your-username/kbase";
          license = licenses.mit;
          mainProgram = "kbase";
        };
      });

    in {
      packages.default = kbase;

      # Additional checks (clippy, tests, etc.)
      checks = {
        inherit kbase;

        kbase-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });

        kbase-fmt = craneLib.cargoFmt { inherit src; };
      };

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
          gcc
          clang
          libclang.lib
          openssl
          pkg-config

          # Embeddings
          ollama

          # TypeScript/Deno ecosystem
          deno
          nodejs
        ];

        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
        RUST_MIN_STACK = "16777216";

        shellHook = ''
          echo "kg development environment ready"
          echo "Use: nix develop"
        '';
      };
    });
}
