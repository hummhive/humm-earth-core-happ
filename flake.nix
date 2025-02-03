{
  description = "Template for Holochain app development";

  inputs = {
    holonix.url = "github:holochain/holonix/main-0.4";
    holonix.inputs.holochain.url = "github:holochain/holochain?ref=holochain-0.4.0-rc.0";
    holonix.inputs.lair-keystore.url = "github:holochain/lair?ref=lair_keystore-v0.5.2";

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";

    hds-releases.url = "github:holo-host/hds-releases";
    
    p2p-shipyard.url = "github:darksoil-studio/p2p-shipyard/main-0.4";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = builtins.attrNames inputs.holonix.devShells;
      perSystem = { inputs', config, pkgs, system, ... }: {
        devShells.default = pkgs.mkShell {
          inputsFrom = [ 
            inputs'.p2p-shipyard.devShells.holochainTauriDev 
            inputs'.holonix.devShells.default
            # inputs'.hds-releases.packages.holo-dev-server-bin
          ];

          packages = (with pkgs; [
            inputs'.hds-releases.packages.holo-dev-server-bin
          ]);
        };
        devShells.androidDev = pkgs.mkShell {
          inputsFrom = [ 
            inputs'.p2p-shipyard.devShells.holochainTauriAndroidDev 
            inputs'.holonix.devShells.default
          ];
        };
      };
    };
}