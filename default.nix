{ pkgs ? import <nixpkgs> { 
    overlays = [ 
      (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/2e7ccf572ce0f0547d4cf4426de4482936882d0e.tar.gz"))
    ];
  },
  rustPlatform ? pkgs.makeRustPlatform { rustc = pkgs.rust-bin.stable.latest.default; cargo = pkgs.rust-bin.stable.latest.default; },
}:
let
  fs =  (pkgs.buildPackages.callPackage ./freeswitch { });
in
rustPlatform.buildRustPackage rec {  
  pname = "freeswitch_rs";
  version = "0.1";
  CFLAGS_COMPILE = "-I${fs.out}/include/freeswitch";
  NIX_CFLAGS_COMPILE="-I${fs.out}/include/freeswitch";

  nativeBuildInputs = with pkgs; [ 
    rustPlatform.bindgenHook
    fs
    pkg-config
  ] ++ lib.optionals stdenv.isDarwin [
  ];

  cargoLock.lockFile = ./Cargo.lock;

  src = pkgs.lib.cleanSource ./.;
  shellHook = ''
    export BINDGEN_EXTRA_CLANG_ARGS="$BINDGEN_EXTRA_CLANG_ARGS $(pkg-config --cflags freeswitch)"
  '';
}
