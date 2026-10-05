{
  description = "GeoSolver - olympiad geometry prover (DDAR) with web app, MCP server and CLI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        geosolver = pkgs.rustPlatform.buildRustPackage {
          pname = "geosolver";
          version = "0.1.0";
          src = nixpkgs.lib.cleanSourceWith {
            src = self;
            filter =
              path: type:
              let
                base = baseNameOf path;
              in
              !(builtins.elem base [
                "target"
                "docs"
                "deploy"
              ]);
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];
          meta = {
            description = "Olympiad geometry prover: web app, MCP server and CLI";
            license = pkgs.lib.licenses.asl20;
            mainProgram = "agstudio";
          };
        };
        default = geosolver;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
          RUST_BACKTRACE = "1";
        };
      });
    };
}
