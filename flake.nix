{
  description = "UEFI InputModules application for Framework Laptop 16";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Read toolchain from rust-toolchain.toml (like uefi-rs does)
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Create a custom rustPlatform with our toolchain
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # Common build inputs
        commonBuildInputs = with pkgs; [
          openssl
          libgit2
          zlib
        ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
          pkgs.darwin.apple_sdk.frameworks.Security
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
        ];

        commonNativeBuildInputs = with pkgs; [
          pkg-config
          zlib # Required by framework_lib build script at runtime
        ];

        # Filter source to only include files needed for the build
        # Using an allowlist ensures new files don't accidentally affect the binary hash
        buildSrc = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              baseName = baseNameOf path;
              relativePath = pkgs.lib.removePrefix (toString ./. + "/") path;
              # Only include files/folders needed for the Rust build
              includedRoots = [
                "src"
                "resources"
                ".cargo"
              ];
              includedFiles = [
                "Cargo.toml"
                "Cargo.lock"
                "build.rs"
                "rust-toolchain.toml"
              ];
              isIncludedRoot = builtins.any (root:
                relativePath == root || pkgs.lib.hasPrefix (root + "/") relativePath
              ) includedRoots;
            in
            isIncludedRoot || builtins.elem baseName includedFiles;
        };

        # Build function for different configurations
        buildInputmodules = { release ? false, features ? [] }:
          let
            profile = if release then "release" else "debug";
            featuresStr = if features == [] then "" else "--features ${builtins.concatStringsSep "," features}";
          in
          rustPlatform.buildRustPackage {
            pname = "inputmodules";
            version = "0.3.16";

            src = buildSrc;

            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                "framework_lib-0.4.3" = "sha256-gO7ieyVQzK3BkNsy451WNtrx51jlzd6vhwJX8RDoj/o=";
                "guid-create-0.4.1" = "sha256-tf38uNtC8YhDN4TZ3yfj4oOG3mYccHPa962YCLkVx4g=";
                "redox_hwio-0.1.6" = "sha256-knLIZ7yp42SQYk32NGq3SUGvJFVumFhD64Njr5TRdFs=";
                "smbios-lib-0.9.1" = "sha256-3L8JaA75j9Aaqg1z9lVs61m6CvXDeQprEFRq+UDCHQo=";
                "uefi-0.20.0" = "sha256-2lUd2a+7NvS94LyAHE2BwGV4j6607mbPXE5htrwdz04=";
              };
            };

            buildType = profile;
            buildNoDefaultFeatures = true;

            # Disable cargo-auditable as it's incompatible with UEFI linker
            auditable = false;

            # Target for UEFI - passed via args to avoid affecting build scripts
            buildPhase = ''
              runHook preBuild
              cargo build \
                ${if release then "--release" else ""} \
                --target x86_64-unknown-uefi \
                ${featuresStr}
              runHook postBuild
            '';

            # Skip check phase - UEFI binaries can't be tested on host
            doCheck = false;

            installPhase = ''
              runHook preInstall
              mkdir -p $out/bin
              cp target/x86_64-unknown-uefi/${profile}/inputmodules.efi $out/bin/
              runHook postInstall
            '';

            nativeBuildInputs = commonNativeBuildInputs;
            buildInputs = commonBuildInputs;

            # Environment variables for C library bindings
            OPENSSL_NO_VENDOR = "1";
            LIBGIT2_NO_VENDOR = "1";

            # Reproducible build flag (matches .cargo/config.toml)
            CARGO_TARGET_X86_64_UNKNOWN_UEFI_RUSTFLAGS = "-Clink-arg=/Brepro";
          };

        # Package definitions
        inputmodules-debug = buildInputmodules { release = false; };
        inputmodules-release = buildInputmodules { release = true; };
        inputmodules-qemu = buildInputmodules { release = false; features = [ "qemu" ]; };
        inputmodules-qemu-release = buildInputmodules { release = true; features = [ "qemu" ]; };

        # OVMF vars config for 2560x1600 resolution (Framework Laptop 16 display)
        ovmfVarsJson = ./ovmf_vars.json;

        # Wrapper script to run the QEMU build in an emulator
        run-qemu = pkgs.writeShellScriptBin "run-inputmodules-qemu" ''
          set -e

          # Create temporary directory for ESP and OVMF vars
          TMPDIR=$(mktemp -d)
          trap "rm -rf $TMPDIR" EXIT

          # Set up ESP filesystem structure
          mkdir -p "$TMPDIR/esp/efi/boot"
          cp ${inputmodules-qemu}/bin/inputmodules.efi "$TMPDIR/esp/efi/boot/bootx64.efi"

          # Copy OVMF_VARS.fd to temp (it needs to be writable)
          cp ${pkgs.OVMF.fd}/FV/OVMF_VARS.fd "$TMPDIR/OVMF_VARS.fd"
          chmod 644 "$TMPDIR/OVMF_VARS.fd"

          # Set 2560x1600 resolution (Framework Laptop 16 display)
          ${pkgs.python3Packages.virt-firmware}/bin/virt-fw-vars \
            --inplace "$TMPDIR/OVMF_VARS.fd" \
            --set-json ${ovmfVarsJson}

          echo "Starting QEMU with InputModules UEFI app..."
          echo "Resolution: 2560x1600"
          echo "Debug output will be written to: $TMPDIR/debug.log"

          ${pkgs.qemu}/bin/qemu-system-x86_64 \
            ''${QEMU_KVM:+-enable-kvm} \
            -M q35 \
            -m 1024 \
            -net none \
            -serial stdio \
            -debugcon "file:$TMPDIR/debug.log" -global isa-debugcon.iobase=0x402 \
            -usb \
            -drive if=pflash,format=raw,readonly=on,file=${pkgs.OVMF.fd}/FV/OVMF_CODE.fd \
            -drive if=pflash,format=raw,readonly=off,file="$TMPDIR/OVMF_VARS.fd" \
            -drive format=raw,file=fat:rw:"$TMPDIR/esp" \
            "$@"
        '';

        # Wrapper for release QEMU build
        run-qemu-release = pkgs.writeShellScriptBin "run-inputmodules-qemu" ''
          set -e

          TMPDIR=$(mktemp -d)
          trap "rm -rf $TMPDIR" EXIT

          mkdir -p "$TMPDIR/esp/efi/boot"
          cp ${inputmodules-qemu-release}/bin/inputmodules.efi "$TMPDIR/esp/efi/boot/bootx64.efi"

          cp ${pkgs.OVMF.fd}/FV/OVMF_VARS.fd "$TMPDIR/OVMF_VARS.fd"
          chmod 644 "$TMPDIR/OVMF_VARS.fd"

          # Set 2560x1600 resolution (Framework Laptop 16 display)
          ${pkgs.python3Packages.virt-firmware}/bin/virt-fw-vars \
            --inplace "$TMPDIR/OVMF_VARS.fd" \
            --set-json ${ovmfVarsJson}

          echo "Starting QEMU with InputModules UEFI app (release)..."
          echo "Resolution: 2560x1600"

          ${pkgs.qemu}/bin/qemu-system-x86_64 \
            ''${QEMU_KVM:+-enable-kvm} \
            -M q35 \
            -m 1024 \
            -net none \
            -serial stdio \
            -debugcon "file:$TMPDIR/debug.log" -global isa-debugcon.iobase=0x402 \
            -usb \
            -drive if=pflash,format=raw,readonly=on,file=${pkgs.OVMF.fd}/FV/OVMF_CODE.fd \
            -drive if=pflash,format=raw,readonly=off,file="$TMPDIR/OVMF_VARS.fd" \
            -drive format=raw,file=fat:rw:"$TMPDIR/esp" \
            "$@"
        '';

      in
      {
        checks = {
          inherit inputmodules-release;
        };

        packages = {
          default = inputmodules-release;
          release = inputmodules-release;
          debug = inputmodules-debug;
          qemu = inputmodules-qemu;
          qemu-release = inputmodules-qemu-release;
          run-qemu = run-qemu;
          run-qemu-release = run-qemu-release;
        };

        # Convenience apps for `nix run`
        apps = {
          default = flake-utils.lib.mkApp { drv = run-qemu; };
          qemu = flake-utils.lib.mkApp { drv = run-qemu; };
          qemu-release = flake-utils.lib.mkApp { drv = run-qemu-release; };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain

            # Development tools
            gnumake
            qemu
            parted
            OVMF
            python3Packages.virt-firmware

            # Build dependencies
            pkg-config
            openssl
            libgit2
            zlib
          ];

          OPENSSL_NO_VENDOR = "1";
          LIBGIT2_NO_VENDOR = "1";

          # Set up OVMF symlinks for `make run` compatibility
          shellHook = ''
            if [ ! -d ovmf ] || [ ! -e ovmf/OVMF_CODE.fd ] || [ ! -e ovmf/OVMF_VARS.fd ]; then
              mkdir -p ovmf
              ln -sf ${pkgs.OVMF.fd}/FV/OVMF_CODE.fd ovmf/OVMF_CODE.fd
              # OVMF_VARS needs to be writable, so copy it instead of symlinking
              cp -f ${pkgs.OVMF.fd}/FV/OVMF_VARS.fd ovmf/OVMF_VARS.fd
              chmod 644 ovmf/OVMF_VARS.fd
              # Set 2560x1600 resolution (Framework Laptop 16 display)
              ${pkgs.python3Packages.virt-firmware}/bin/virt-fw-vars \
                --inplace ovmf/OVMF_VARS.fd \
                --set-json ovmf_vars.json
              echo "OVMF files set up in ovmf/ (2560x1600 resolution)"
            fi
          '';
        };
      }
    );
}
