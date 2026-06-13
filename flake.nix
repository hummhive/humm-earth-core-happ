{
  description = "Template for Holochain app development";

  inputs = {
    holonix.url = "github:holochain/holonix/main-0.6";

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";

    hds-releases.url = "github:holo-host/hds-releases";

    # p2p-shipyard disabled - many dependencies are now private repos
    # For Tauri/Android builds, contact darksoil-studio for access
    # p2p-shipyard.url = "github:darksoil-studio/p2p-shipyard/main-0.6";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = builtins.attrNames inputs.holonix.devShells;
      perSystem = { inputs', config, pkgs, system, ... }: {
        # Zome-only dev shell (no Tauri/Android support)
        # Sufficient for building .happ files for EdgeNode deployment
        devShells.default = pkgs.mkShell {
          inputsFrom = [
            inputs'.holonix.devShells.default
          ];

          packages = (with pkgs; [
            inputs'.hds-releases.packages.holo-dev-server-bin
            binaryen # wasm-opt for scripts/strip-wasms.sh — see .baseline-hashes.txt "Reproducibility contract"
          ]);
        };

        # Note: androidDev shell requires p2p-shipyard access
        # Contact darksoil-studio for enterprise/commercial access
      };
    };
}