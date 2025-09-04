{
  rustPlatform
, pkg-config
, freeswitch
, lib
}:
rustPlatform.buildRustPackage rec {  
  pname = "freeswitch_rs_mods";
  version = "0.1";

  nativeBuildInputs = [ 
    rustPlatform.bindgenHook
    pkg-config
  ];

  cargoLock.lockFile = ./Cargo.lock;

  src = lib.cleanSource ./.;
}
