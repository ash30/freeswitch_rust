let pkgs = import (builtins.fetchTarball https://github.com/NixOS/nixpkgs/archive/nixpkgs-unstable.tar.gz) {
  config.allowUnfree = true; 
  overlays = [ 
    (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"))
  ];
};
  rustc = pkgs.rust-bin.stable.latest.default.override { extensions = ["rust-src"];};
  cargo = pkgs.rust-bin.stable.latest.default;
  rustPlatform = pkgs.makeRustPlatform { rustc = rustc; cargo = cargo;};
in
pkgs.mkShell {
  inputsFrom = [ (pkgs.callPackage ./default.nix { inherit rustPlatform; }) ];
  buildInputs = [
    (import (builtins.fetchGit { url = "https://github.com/ash30/freeswitch"; ref =  "github_action_debug";  rev = "69821b1c0dce77daba8532e4baef92edc8f3a852";}) {})
    pkgs.pkg-config
    pkgs.rust-bin.stable.latest.rust-analyzer # LSP Server
    pkgs.rust-bin.stable.latest.rustfmt       # Formatter
    pkgs.rust-bin.stable.latest.clippy        # Linter
  ];
  RUST_SRC_PATH = "${rustc}/lib/rustlib/src/rust/library/";
}
