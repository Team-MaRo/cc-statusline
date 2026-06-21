{
  description = "Customizable Claude Code statusline with rate-limit curves, context bar, and git info";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    # Shared OCI helpers (createdFromDate, fixOciImageHistory). `follows` keeps
    # a duplicate nixpkgs out of flake.lock.
    nix-utils.url = "github:Team-MaRo/nix-utils";
    nix-utils.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , nix-utils
    }:
    let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      mkPackage = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "cc-statusline";
          version = cargoToml.package.version;

          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # chrono-tz bundles the full IANA db; filter to the only zone we use
          # (peak-hours math in src/peak.rs). Mirrors .cargo/config.toml, set
          # here too so the build is correct regardless of cargo config merging.
          env.CHRONO_TZ_TIMEZONE_FILTER = "America/Los_Angeles";

          # cc-statusline shells out to `git` (src/git.rs); make it available on
          # PATH even under a minimal environment.
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/cc-statusline \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git ]}
          '';

          meta = {
            description = cargoToml.package.description;
            homepage = "https://github.com/Team-MaRo/cc-statusline";
            license = pkgs.lib.licenses.mit;
            mainProgram = "cc-statusline";
          };
        };

      # Build an OCI image tarball for `cc-statusline` (Linux only). Mirrors the
      # streamLayeredImage + nix-utils history-fixer pattern used across
      # Team-MaRo repos. The image is a one-shot CLI: entrypoint is the binary,
      # format strings are passed as `docker run` args.
      mkDockerImage = pkgs: cc-statusline:
        let
          # docker/metadata-action labels (KEY=VAL\n…) serialised to JSON by the
          # workflow and read here. Empty when built locally.
          labelsJson = builtins.getEnv "DOCKER_LABELS_JSON";
          labels = if labelsJson == "" then { } else builtins.fromJSON labelsJson;

          inherit (nix-utils.lib.oci) createdFromDate;
          fixHistoryScript = nix-utils.packages.${pkgs.stdenv.hostPlatform.system}.fixOciImageHistory;

          dockerImageStream = pkgs.dockerTools.streamLayeredImage {
            name = "cc-statusline"; # cosmetic; the workflow retags to vars.IMAGE_NAME
            tag = "latest";

            # Flake's last-modified date (HEAD commit time on a clean tree) so
            # identical sources produce an identical config digest. `"now"`
            # would change every build.
            created = createdFromDate self.lastModifiedDate;

            contents = [
              pkgs.dockerTools.usrBinEnv
              # /etc/passwd + /etc/group with a nonroot (65532) user.
              (pkgs.dockerTools.fakeNss.override {
                extraPasswdLines = [ "nonroot:x:65532:65532:nonroot:/tmp:/sbin/nologin" ];
                extraGroupLines = [ "nonroot:x:65532:" ];
              })
              # The wrapped binary pulls git + bash into the closure transitively.
              cc-statusline
            ];

            # uid 65532 needs a writable /tmp (dockerTools dirs default to 755).
            extraCommands = "mkdir -p tmp && chmod 1777 tmp";
            enableFakechroot = true;

            config = {
              User = "65532:65532";
              WorkingDir = "/work";
              # Persist per-session state under a writable dir (best-effort).
              Env = [ "XDG_CACHE_HOME=/tmp" ];
              Entrypoint = [ (pkgs.lib.getExe cc-statusline) ];
              Labels = labels;
            };
          };
        in
        {
          inherit dockerImageStream;
          dockerImage = pkgs.runCommand "cc-statusline-image.tar" { } ''
            ${dockerImageStream} | ${fixHistoryScript} > $out
          '';
        };
    in
    flake-utils.lib.eachSystem (flake-utils.lib.defaultSystems ++ [ "riscv64-linux" ])
      (system:
        let
          pkgs = import nixpkgs { inherit system; };
          cc-statusline = mkPackage pkgs;
          docker = if pkgs.stdenv.isLinux then mkDockerImage pkgs cc-statusline else { };
        in
        {
          packages = {
            default = cc-statusline;
            cc-statusline = cc-statusline;
          } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
            inherit (docker) dockerImage dockerImageStream;
          };

          apps.default = flake-utils.lib.mkApp { drv = cc-statusline; };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ cc-statusline ];
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
            ];
          };

          formatter = pkgs.nixpkgs-fmt;
        })
    // {
      # Consumers: nixpkgs.overlays = [ cc-statusline.overlays.default ];
      overlays.default = final: prev: {
        cc-statusline = mkPackage final;
      };
    };
}
