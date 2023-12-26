{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      perSystem = { pkgs, config, inputs', system, lib, ... }: {
        formatter = pkgs.nixpkgs-fmt;

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.just
            pkgs.watchexec
            pkgs.fd

            # formatters
            pkgs.taplo
            pkgs.nodePackages.prettier
            config.formatter
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
