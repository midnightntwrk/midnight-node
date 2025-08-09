{
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      isDarwin = pkgs.lib.hasSuffix "darwin" system;
      isDarwinAArch64 = system == "aarch64-darwin";
    in {
      devShells.default = let rust = [];

      in pkgs.mkShell {
        packages = with pkgs; [
           earthly rustup clang pkg-config zlib
        ] ++ (if isDarwin
          then with pkgs.darwin; [ libiconv apple_sdk.frameworks.SystemConfiguration apple_sdk.frameworks.Security ]
          else []);
        buildInputs = [ pkgs.libclang ];
        WASM_BUILD_STD=0;
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        PROTOC = "${pkgs.protobuf}/bin/protoc";
        ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = with pkgs;
          if isDarwinAArch64
            then "-isystem ${darwin.apple_sdk.Libsystem}/include" else "";
        shellHook = ''
          . ./.envrc
        '';
      };
    });
}
