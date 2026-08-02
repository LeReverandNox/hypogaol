{
  description = "hypogaol development environment";

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

            # Runtime tools hypogaol orchestrates (AD-1) — not build deps,
            # only needed locally to run the hardware-gated integration suite (AD-7).
            cryptsetup
            systemd
            libfido2
            psmisc # fuser, slam's busy-mount escalation (AD-18); kill itself
                   # comes from util-linux-minimal, already pulled in
                   # transitively (confirmed via `nix develop`, 2026-07-28)
            lvm2 # dmsetup, close-all's live mapping discovery (AD-17)
          ];

          RUST_BACKTRACE = "1";
        };
      });
}
