final: prev: {
    rust-bindgen-unwrapped = prev.rust-bindgen-unwrapped.overrideAttrs rec { 
     version = "main";
     src = prev.fetchFromGitHub {
       owner  = "rust-lang";
       repo   = "rust-bindgen";
       rev    = "66b65517b5568e122e9ce5902dd4868aa2b43d25";
       sha256 = "sha256-aXF6nR3DpeH3o05uyhaa3s8fJF6JUGs/J9bvQz0LGSs=";
     };
     cargoDeps = prev.rustPlatform.fetchCargoTarball({ 
	inherit src; 
	hash = "sha256-Pqnx+9Oa9ypRQDdhwIQ8XlPm8WAeg4CvEr7/sFyMWCI=";
     });
    };
}
