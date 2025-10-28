{
  inputs,
  targetSystem,
}: let
  pkgs = inputs.nixpkgs.legacyPackages.${targetSystem};
  inherit (pkgs) lib;

  fenix = inputs.fenix.packages.${pkgs.system};

  # A toolchain with the wasm32 target available:
  rustToolchain = fenix.combine [
    fenix.stable.toolchain
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
    fenix.stable.rust-src
    fenix.stable.llvm-tools
  ];

  craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

  src = lib.cleanSourceWith {
    src = lib.cleanSource ../.;
    filter = path: type:
      craneLib.filterCargoSources path type
      || lib.hasSuffix ".scale" path
      || lib.hasSuffix ".mn" path
      || lib.hasSuffix ".json" path
      || lib.hasSuffix "COMPACTC_VERSION" path;
    name = "source";
  };

  packageName = craneLib.crateNameFromCargoToml {cargoToml = builtins.path {path = src + "/node/Cargo.toml";};};

  commonArgs =
    {
      inherit (packageName) version pname;
      inherit src;
      strictDeps = true;

      # FIXME: `frame-storage-access-test-runtime`’s `build.rs` script fails otherwise, it’d be good to fix, see:
      # <https://github.com/paritytech/polkadot-sdk/blob/6fd693e6d9cfa46cd2acbcb41cd5b0451a62d67c/substrate/utils/frame/storage-access-test-runtime/build.rs>
      SKIP_WASM_BUILD = 1;

      nativeBuildInputs =
        [
          pkgs.gnum4
          pkgs.protobuf
        ]
        ++ lib.optionals pkgs.stdenv.isLinux [
          pkgs.pkg-config
        ];
      buildInputs =
        lib.optionals pkgs.stdenv.isLinux [
          pkgs.openssl
        ]
        ++ lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
          pkgs.darwin.apple_sdk_12_3.frameworks.SystemConfiguration
          pkgs.darwin.apple_sdk_12_3.frameworks.Security
          pkgs.darwin.apple_sdk_12_3.frameworks.CoreFoundation
        ];
    }
    // lib.optionalAttrs pkgs.stdenv.isLinux {
      # The linker bundled with Fenix has wrong interpreter path, and it fails with ENOENT, so:
      RUSTFLAGS = "-Clink-arg=-fuse-ld=bfd";
      # The same problem for the Wasm linker:
      CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER = "${pkgs.llvmPackages.lld}/bin/wasm-ld";
    }
    // lib.optionalAttrs pkgs.stdenv.isDarwin {
      # for bindgen, used by libproc, used by metrics_process
      LIBCLANG_PATH = "${lib.getLib pkgs.llvmPackages.libclang}/lib";
    };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  package = craneLib.buildPackage (commonArgs
    // {
      inherit cargoArtifacts;
      doCheck = false; # we run tests elsewhere
    });
in
  package
