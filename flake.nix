{
  # Development shell for Norn. Optional convenience: everything here can
  # also be installed with a normal package manager (see the README).
  description = "Norn development shell (Rust, Python, pytest, hyperfine)";

  # Pinned to one nixpkgs commit (the one the development machine runs), so the
  # tools are the same every time and most of them are already on disk.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/c69ae8fb8faeb3472fd11234ba55a70ac3601f9b";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          pkgs.cargo
          pkgs.rustc
          pkgs.rustfmt
          pkgs.clippy
          # The demo repo's tests run with pytest inside the sandbox.
          (pkgs.python3.withPackages (ps: [ ps.pytest ]))
          pkgs.hyperfine
          pkgs.diffutils
          pkgs.gnumake
        ];
        # rusqlite is built with its bundled SQLite, so no system SQLite is needed.
      };
    };
}
