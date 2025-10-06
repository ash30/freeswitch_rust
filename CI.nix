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
    (import (builtins.fetchGit { url = "https://github.com/ash30/freeswitch"; ref =  "nix";  rev = "d805600c0f4843528290a0d018bb1fbb5b56808f";}) {})
    pkgs.pkg-config
    pkgs.rust-bin.stable.latest.rustfmt       # Formatter
    pkgs.rust-bin.stable.latest.clippy        # Linter
  ];
}
