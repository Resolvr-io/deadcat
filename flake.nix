{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        tauriCli =
          if pkgs ? tauri-cli then pkgs.tauri-cli
          else if pkgs ? cargo-tauri then pkgs.cargo-tauri
          else null;
        chromium =
          if lib.meta.availableOn pkgs.stdenv.hostPlatform pkgs.chromium
          then pkgs.chromium
          else null;

      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            clippy
            rustfmt
            git
            nodejs_20
            pnpm
            just
            rustc
            typescript
            sqlite
            pkg-config
            biome
          ]
          ++ lib.optional (tauriCli != null) tauriCli
          ++ lib.optional (chromium != null) chromium;
          shellHook = lib.optionalString (chromium != null) ''
            export PUPPETEER_EXECUTABLE_PATH="${chromium}/bin/chromium"
          '';
        };
      });
}
