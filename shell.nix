let pkgs = import (builtins.fetchTarball {
  # Descriptive name to make the store path easier to identify
  name = "nixpkgs-unstable";
  # commit hash for 23.05
  url = "https://github.com/nixos/nixpkgs/archive/154bcb95ad51bc257c2ce4043a725de6ca700ef6.tar.gz";
  # Hash obtained using `nix-prefetch-url --unpack <url>`
  sha256 = "0gv8wgjqldh9nr3lvpjas7sk0ffyahmvfrz5g4wd8l2r15wyk67f";
}) { localSystem = "aarch64-darwin"; };
freeswitch =  (pkgs.callPackage ./ext/freeswitch { });
in
pkgs.mkShell rec {
  nativeBuildInputs = [
    pkgs.rust-analyzer
    pkgs.clang
    pkgs.cmake
    pkgs.libclang
    pkgs.rustfmt
    pkgs.llvmPackages.bintools
    pkgs.rustup
    pkgs.rust-bindgen
    pkgs.darwin.apple_sdk.frameworks.Security
    pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
    pkgs.pkg-config
    pkgs.openssl
    pkgs.cargo-watch
    pkgs.darwin.libiconv

    freeswitch 
  ];

  RUSTFLAGS = (builtins.map (a: ''-L${a}/lib'')[
    pkgs.libiconv
  ]);

  PROJECT_ROOT = builtins.getEnv "PWD";
  RUSTC_VERSION = pkgs.lib.readFile "${PROJECT_ROOT}/rust-toolchain";
  
  shellHook = ''
  export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin
  export PATH=$PATH:''${RUSTUP_HOME:-~/.rustup}/toolchains/$RUSTC_VERSION-x86_64-unknown-linux-gnu/bin/
  export NIX_CFLAGS_COMPILE="$(pkg-config --cflags freeswitch) $NIX_CFLAGS_COMPILE"
  '';
}
