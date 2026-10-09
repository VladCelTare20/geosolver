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
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
      revision = self.shortRev or self.dirtyShortRev or "unknown";
      geosolverWith =
        pkgs: doCheck:
        pkgs.rustPlatform.buildRustPackage {
          pname = "geosolver";
          inherit version doCheck;
          src = nixpkgs.lib.cleanSourceWith {
            src = self;
            filter =
              path: _:
              let
                base = baseNameOf path;
                rel = nixpkgs.lib.removePrefix "${toString self}/" (toString path);
              in
              !(builtins.elem base [
                "target"
                "docs"
                "nix"
                "flake.nix"
                "flake.lock"
              ])
              && (rel == "deploy" || rel == "deploy/.env.example" || !(nixpkgs.lib.hasPrefix "deploy" rel));
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "--workspace" ];
          GEOSOLVER_REVISION = revision;
          meta = {
            description = "Olympiad geometry prover: web app, MCP server and CLI";
            homepage = "https://github.com/VladCelTare20/geosolver";
            license = pkgs.lib.licenses.asl20;
            mainProgram = "agstudio";
            platforms = systems;
          };
        };
    in
    {
      packages = forAll (pkgs: rec {
        geosolver = geosolverWith pkgs false;
        default = geosolver;
      });

      checks = forAll (pkgs: {
        tests = geosolverWith pkgs true;
      });

      nixosModules.default = import ./nix/module.nix self;

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
