# Framework Laptop 16 - UEFI Inputmodule App

This UEFI app shows the current status of inputmodules of the Framework Laptop 16.

It's launched automatically by UEFI during boot when it is detected that
the input deck is not fully populated. In this case some pins are exposed and
it is not fully safe to power the deck on.

This app helps the user see which modules are detected because even though they
might be physically installed, they might not be properly electrically
connected. The app updates whenver a module is installed or removed.
Once all modules are installed, the input deck is supplied with power and booting can continue.

In some cases, for example standalone operation of the mainboard, it is
desirable to skip this safety check. Because a keyboard might not be connected
there is no key shortcut to press.
Implementation of a mechanism to skip is pending implementation.

Conditions to boot - until one of the following conditions is met, the app runs
in a loop and keeps checking the current status:

- Press ESC
- Inputmodules fully populated, so that the whole surface is covered
- Input deck state overridden from EC console
- (NOT IMPLEMENTED YET: Short-Press power button)

## Running

Framework Laptop 16 BIOS will ship with this app embedded.

To run a modified version, build it, as below, or download a release binary.

It can be started on the shell, just by starting the executable.
For debugging, providing any commandline arguments displays the inputmodule
status on the commandline:

```
# Launch GUI
Shell> fs0:inputmodules.efi
Shell> fs0:inputmodules.efi gui

# Launch GUI even if everything is okay
Shell> fs0:inputmodules.efi force

# Show debug
Shell> fs0:inputmodules.efi info
Input Deck State: On
Touchpad present: true
Positions:
  Pos 0: KeyboardA
  Pos 1: Disconnected
  Pos 2: Disconnected
  Pos 3: KeyboardB
  Pos 4: Disconnected
```

Once in the GUI you can use ESC to quit.

## Building

Pre-requisites:

- [Rustup](https://rustup.rs/) (MSRV: 1.61)

Build:

```
# Build debug version
> make
-rwxrwxr-x 2 zoid zoid 695K May  2 11:48 target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi

# Build release version
> make release
-rwxrwxr-x 2 zoid zoid 426K May  2 11:49 target/x86_64-unknown-uefi/release/uefi-qmk-hid.efi

# Copy executable to UEFI System Partition (ESP)
cp target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi /boot/efi

# Make sure to also put a UEFI shell at /boot/efi/EFI/BOOT/BOOTX64.efi
# Or put it somewhere else on the ESP and add a bootloader entry for it
```

## Building with Nix

For reproducible builds across different systems, you can use [Nix](https://nixos.org/).

### Building

```sh
# Build release version (default)
nix build

# Build debug version
nix build .#debug

# Build with QEMU feature
nix build .#qemu
nix build .#qemu-release
```

The resulting binary will be at `result/bin/inputmodules.efi`.

### Running in QEMU

Run directly in QEMU with the Framework Laptop 16 native resolution (2560x1600):

```sh
# Debug build
nix run

# Release build
nix run .#qemu-release
```

### Development Shell

Enter a development shell with all tools (Rust toolchain, QEMU, OVMF, etc.):

```sh
nix develop
```

This sets up:
- Rust toolchain with the `x86_64-unknown-uefi` target
- QEMU and OVMF for testing
- `virt-fw-vars` for UEFI variable manipulation
- OVMF files in `ovmf/` configured for 2560x1600 resolution

You can then use `make` commands as usual:

```sh
make qemu && make run
```

## Debugging
To test in QEMU build the QEMU version and run it.

Pre-reqs:

- QEMU
- Need to put `OVMF_CODE.fd` and `OVMF_VARS.fd` in the root folder (if it doesn't work automatically).

```sh
# Debug version
make qemu && make run

# Release version
make qemu-release && make run TARGET=release
```

You can cycle through different amounts of population by pressing the numbers 0
through 5. ESC quits the app.

###### Running in native Framework Laptop 16 resolution

When using Nix (`nix run` or `nix develop`), the 2560x1600 resolution is
configured automatically via `ovmf_vars.json`.

For manual setup, QEMU/OVMF has the capability to display higher resolutions.
During early startup, press ESC repeatedly until you enter the setup menu.
There navigate to "Device Manager" -> "OVMF Platform Configuration" and change
the preferred resolution to 2560x1600.

Upon saving and rebooting, QEMU should launch in that resolution. Most likely
it will not fit on your screen, so you'll have to make it smaller from the
window menu: "View" -> "Zoom Out".

## Behavior and Design Guidelines

- Basics
  - MUST not panic
  - SHOULD not do any logging (except maybe errors) in release build
  - SHOULD save as much space as possible in binary
  - MUST exit immediately if everything is okay on application start
  - SHOULD avoid a tight busyloop to avoid running the CPU too hot
  - SHOULD show input modules in their correct position
- UEFI Interaction
  - MUST clear screen to black on exit otherwise it might only be partially redrawn by the bootloader
  - MUST disable UEFI App mode on exit so that regular power button behavior is restored
- User Guide
  - When touchpad is installed but any top-row module isn't, show how to remove touchpad
  - We don't want to allow booting without Expansion Bay Module (because no fan)
    - But we at least need to allow user to get into setup menu to enable standalone mode
- Limitations
  - Can't detect exactly which input module is present because they're not powered on until all are installed
    - But different sizes have different resistors, so we can at least detect the size (and KB vs no KB)
  - Can't detect touchpad position, all locations are wired together

## QEMU Special Keys

Controls:

- A, S, D, F, G: AC/Battery states (No Battery, 5%, 5% charging, 20%, 20% charging)
- Q, W, E: Expansion Bay selection (No Module, bad connection, dual)
- Z, X, C, V: Fan selection (No Fan, Left, Right, Both)
- L: Interposer Door/Latch toggle
- H: Hubboard toggle
- 1, 2, 3, 4, 5, 6, 7: Inputmodules (Only Touchpad, None, 1, 2, 3, 3+Touchpad, Full-Width+Touchpad)
- P: Outdated firmware warning toggle (keyboard + PD)
- B: Device diagnostics complete toggle

Finish everything: L, V, E, H, 7, P, B

## Security Design

This sections explains the design of this project.
It should serve as guidance for future changes. Any changes that change the
security related design, should be carefully evaluated.

To limit how bugs in this application affect boot-time and overall system
security, this app/driver is constrained to a very limited purpose and does not
have many touchpoints with anything else.

Security boundaries:

- does **not** read/write to/from disk
- does **not** interact with the network
- does **not** leave anything in memory after it exits
- **may** access (read/write) UEFI variables (to communicate issues to the OS)
- does **not** execute any UEFI drivers/executables
- does **not** interact with USB or other external devices

It has **one** purpose: detect issues with the system and show a graphical
interface to the user to let them know what is wrong and what to do about it.
