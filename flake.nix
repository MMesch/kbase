{
  description = "kbase - Knowledge Graph CLI for markdown notes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
    in {
      packages.default = pkgs.rustPlatform.buildRustPackage {
        pname = "kbase";
        version = "0.1.0";
        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

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

        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

        # Use clang for C++ to avoid gcc 15 ICE
        CXX = "${pkgs.clang}/bin/clang++";
        CC = "${pkgs.clang}/bin/clang";

        # Wrap binary to load ONNX Runtime at runtime
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

        shellHook = ''
          echo "kg development environment ready"
          echo "Use: nix develop"
        '';
      };
    });
}
