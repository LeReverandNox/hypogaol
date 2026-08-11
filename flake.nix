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
            cargo-llvm-cov
            cargo-audit

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

          # cargo-llvm-cov looks for llvm-tools-preview under rustc's own sysroot
          # (a rustup convention); nixpkgs's rustc has no such component, so point
          # it at the matching-version LLVM tools explicitly (confirmed matching
          # via `rustc.llvmPackages.llvm.version` == `rustc --version --verbose`'s
          # reported LLVM version, 2026-08-11).
          LLVM_COV = "${pkgs.rustc.llvmPackages.llvm}/bin/llvm-cov";
          LLVM_PROFDATA = "${pkgs.rustc.llvmPackages.llvm}/bin/llvm-profdata";
        };
      });
}
