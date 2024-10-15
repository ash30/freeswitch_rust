{ pkgs ? import <nixpkgs> { 
    overlays = [ 
       # https://github.com/oxalica/rust-overlay/commit/0bf05d8534406776a0fbc9ed8d4ef5bd925b056a
       # Why does this break?
      (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/2e7ccf572ce0f0547d4cf4426de4482936882d0e.tar.gz"))
    ];
  } 
}:
let
  rustPlatform = pkgs.makeRustPlatform {
    rustc = pkgs.rust-bin.stable.latest.default;
    cargo = pkgs.rust-bin.stable.latest.default;
  };
  fs =  (pkgs.buildPackages.callPackage ./freeswitch { });
in
rustPlatform.buildRustPackage rec {  
  pname = "freeswitch_rs";
  version = "0.1";
  nativeBuildInputs = with pkgs; [ 
    rustPlatform.bindgenHook
    fs
  ] ++ lib.optionals stdenv.isDarwin [
  ];
  CFLAGS_COMPILE = "-I${fs.out}/include/freeswitch";
  NIX_CFLAGS_COMPILE="-I${fs.out}/include/freeswitch";

  cargoLock.lockFile = ./Cargo.lock;

  src = pkgs.lib.cleanSource ./.;
  shellHook = ''
    export NIX_CFLAGS_COMPILE=$NIX_CFLAGS_COMPILE:${fs.out}/include/freeswitch
    
  '';
}
