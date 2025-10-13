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
    (import (builtins.fetchGit { url = "https://github.com/ash30/freeswitch"; ref =  "build/github_action_debug";  rev = "e9c3590a5cd247e5b490d9b0e939745bd39d224b";}) {})
    pkgs.pkg-config
    pkgs.rust-bin.stable.latest.rustfmt       # Formatter
    pkgs.rust-bin.stable.latest.clippy        # Linter
  ];
}
