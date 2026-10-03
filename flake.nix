{
  description = "CLI for UniFi Network controller";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        unifi-cli = pkgs.rustPlatform.buildRustPackage {
          pname = "unifi-cli";
          version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false;
          meta = {
            description = "CLI for UniFi Network controller";
            license = pkgs.lib.licenses.mit;
            mainProgram = "unifi";
          };
        };
        default = unifi-cli;
      });

      overlays.default = final: _prev: {
        unifi-cli = self.packages.${final.stdenv.hostPlatform.system}.unifi-cli;
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });
    };
}
