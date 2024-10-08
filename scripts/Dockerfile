# Nix builder
FROM nixos/nix:latest AS builder

RUN nix-prefetch-url https://github.com/nixos/nixpkgs/archive/154bcb95ad51bc257c2ce4043a725de6ca700ef6.tar.gz
RUN nix repl --expr 'import (builtins.fetchTarball { name = "nixpkgs-unstable"; url = "https://github.com/nixos/nixpkgs/archive/154bcb95ad51bc257c2ce4043a725de6ca700ef6.tar.gz"; sha256 = "0gv8wgjqldh9nr3lvpjas7sk0ffyahmvfrz5g4wd8l2r15wyk67f";}) { localSystem = "x86_64-linux"; }'

COPY . /tmp/build
WORKDIR /tmp/build

#RUN nix-shell  -vA
# TODO: need to manually link fs repo atm...
RUN nix-build ./freeswitch
