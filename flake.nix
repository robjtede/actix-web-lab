{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      perSystem = { pkgs, config, inputs', system, lib, ... }: {
        formatter = pkgs.nixpkgs-fmt;

        devShells.default = pkgs.mkShell {
          packages = [
            config.formatter
            inputs'.nixpkgs-unstable.legacyPackages.nodePackages.prettier
            pkgs.taplo
            pkgs.just
          ] ++ lib.optional pkgs.stdenv.isDarwin [
            pkgs.pkgsBuildHost.libiconv
            pkgs.pkgsBuildHost.darwin.apple_sdk.frameworks.Security
            pkgs.pkgsBuildHost.darwin.apple_sdk.frameworks.CoreFoundation
            pkgs.pkgsBuildHost.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
        };
      };
    };
}
