{
  description = "tomb-fido2 development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer

            # Runtime tools tomb-fido2 orchestrates (AD-1) — not build deps,
            # only needed locally to run the hardware-gated integration suite (AD-7).
            cryptsetup
            systemd
            libfido2
            psmisc # fuser/kill, slam's busy-mount escalation (AD-18)
            lvm2 # dmsetup, close-all's live mapping discovery (AD-17)
          ];

          RUST_BACKTRACE = "1";
        };
      });
}
