{ pkgs ? import <nixpkgs> { 
    overlays = [ 
       # https://github.com/oxalica/rust-overlay/commit/0bf05d8534406776a0fbc9ed8d4ef5bd925b056a
       # Why does this break?
      (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/2e7ccf572ce0f0547d4cf4426de4482936882d0e.tar.gz"))
    ];
  }
}:
let
  rust = pkgs.rust-bin.stable.latest.default.override { extensions = ["rust-src"]; };
  rustPlatform = pkgs.makeRustPlatform {
    rustc = rust;
    cargo = pkgs.rust-bin.stable.latest.default;
  };
in
pkgs.mkShell {
  inputsFrom = [ (pkgs.callPackage ./default.nix { }) ];
  buildInputs = with pkgs; [
    rust
    rust-bin.stable.latest.rust-analyzer # LSP Server
    rust-bin.stable.latest.rustfmt       # Formatter
    rust-bin.stable.latest.clippy        # Linter
  ];
  RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library/";
}
