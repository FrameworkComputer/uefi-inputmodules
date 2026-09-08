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
- Q, W, E, R: Expansion Bay selection (No Module, bad connection, UMA fan, dual interposer (dGPU))
- Z, X, C, V: Fan selection (No Fan, Left, Right, Both)
- L: Interposer Door/Latch toggle
- H: Hubboard toggle
- 1, 2, 3, 4, 5, 6, 7: Inputmodules (Only Touchpad, None, 1, 2, 3, 3+Touchpad, Full-Width+Touchpad)
- P: Outdated firmware warning toggle (keyboard + PD)
- B: Device diagnostics complete toggle (until pressed, the finished screen shows "Checking fan status..." and fan errors are not reported)

Finish everything: L, V, E, H, 6, P, B

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

## Visual Regression Testing

This project includes an automated visual regression testing system that runs the UEFI app in QEMU, simulates different hardware states, captures screenshots, and compares them against baseline images. This ensures UI changes are intentional and don't break existing functionality.

### How It Works

1. **Test Runner**: A Rust-based tool (`tests/visual/`) that controls QEMU programmatically
2. **QMP Protocol**: Uses QEMU Machine Protocol to send keystrokes and capture screenshots
3. **Scenarios**: YAML files define test sequences (keystrokes, waits, screenshots)
4. **Baselines**: Expected screenshots stored in `tests/visual/baselines/`
5. **Comparison**: Exact pixel-by-pixel comparison for strict validation
6. **Stable frames**: Before every screenshot the runner polls the framebuffer until two
   consecutive frames are identical, so scenarios don't need fixed waits after keypresses
7. **Duplicate detection**: At the end of a run, screenshots that are pixel-identical to each
   other are listed as a warning. Usually that means a keypress had no visible effect or the
   state you wanted to see is masked by a higher-priority message.

The app under test is built with the `visual-test` cargo feature (implies `qemu`, and freezes
dynamic elements like the boot countdown) into its own target directory
`target/visual-test/`, so the runner can never pick up a binary built with other features.

### Prerequisites

- QEMU (`qemu-system-x86_64`)
- OVMF firmware (auto-downloaded or install `edk2-ovmf` / `ovmf` package)

### Running Tests

```bash
# Run all visual tests (uses release build by default)
make test

# Run tests with debug build
make test-debug

# List available test scenarios
make test-list

# Run specific scenarios (substring match on file name or scenario name)
make test FILTER=fan
make test FILTER=battery

# Or invoke the runner directly (after `make visual-test test-build`)
./tests/visual/target/debug/visual-test-runner run --filter "fan"
```

### Updating Baselines

When you intentionally change the UI, update the baseline screenshots:

```bash
# Update all baselines (release build)
make test-update

# Update baselines with debug build
make test-update-debug

# Update specific scenarios only
make test-update FILTER=expansion
```

After updating, review the changes in `tests/visual/baselines/` and commit them.
Check the "pixel-identical screenshots" warning at the end of the run: a new
screenshot that equals an existing one usually means the scenario is not
showing the state its name claims.

### Test Scenarios

Scenarios are defined in `tests/visual/scenarios/` as YAML files:

```yaml
name: "Expansion bay states"
description: "Test expansion bay module detection states"
# Optional: override default build mode (release/debug)
# build_mode: debug

steps:
  # Wait for OVMF to boot the app (the firmware splash is also a stable frame,
  # so this one wait cannot be replaced by stable-frame detection)
  - action: wait
    ms: 2000

  # Simulate hardware state (see QEMU Special Keys above)
  - action: send_keys
    keys: ["q"]  # No expansion bay module

  # Capture and compare screenshot. The runner waits for the frame to stop
  # changing before capturing, so no wait step is needed after send_keys.
  - action: screenshot
    name: "no_expansion_bay"
    compare: true
```

The UI only shows the highest-priority problem, so a scenario has to put every
higher-priority item into an OK state first. The order is: expansion bay /
interposer door / fans (`L`, `V`, `Q`/`W`/`E`/`R`), hubboard (`H`), input
modules and touchpad (`1`-`7`), outdated firmware (`P`), then EC diagnostics
complete (`B`). Without `B` the finished screen shows "Checking fan status..."
and missing fans are not reported.

#### Available Actions

| Action | Parameters | Description |
|--------|------------|-------------|
| `wait` | `ms` | Wait for specified milliseconds (only needed for the initial boot) |
| `send_keys` | `keys` (list) | Send keyboard keys to QEMU |
| `screenshot` | `name`, `compare` | Wait for a stable frame, capture it, optionally compare to baseline |

#### Key Mapping

Keys are mapped to QEMU key codes. Common keys:
- Letters: `a`-`z` (lowercase)
- Numbers: `0`-`9`
- Special: `esc`, `enter`, `space`, `f12`
- Shift combo: `?` (sends shift+slash)

### Writing New Tests

1. Create a new YAML file in `tests/visual/scenarios/`:
   ```bash
   touch tests/visual/scenarios/11_my_new_test.yaml
   ```

2. Define your test scenario (see example above)

3. Generate the baseline:
   ```bash
   make test-update FILTER=my_new_test
   ```

4. Verify the screenshot looks correct in `tests/visual/baselines/` and that it
   is not listed as identical to an existing baseline

5. Run the test to confirm it passes:
   ```bash
   make test FILTER=my_new_test
   ```

### Build Modes

Tests run in **release mode by default** for consistency with production builds. You can override this:

- **CLI flag**: `--debug` or `-d` to use debug build
- **Per-scenario**: Add `build_mode: debug` in the YAML file

When a scenario specifies a different build mode than the current one, the test runner automatically restarts QEMU with the appropriate binary.

### CI Integration

Visual tests run automatically on GitHub Actions. On failure:
- Test outputs (actual screenshots + diff images) are uploaded as artifacts
- Diff images highlight pixel differences in red for easy debugging

### Troubleshooting

**Tests fail with verbose/debug info in screenshots**
- In release mode, verbose is OFF by default (correct for testing)
- In debug mode, verbose is ON by default, which causes non-deterministic output
- Use release mode (`make test`) for consistent results
- The `?` key toggles verbose mode - don't use it in release mode scenarios

**QEMU fails to start**
- Check that OVMF firmware is available: `make ovmf/OVMF_CODE.fd`
- Ensure QEMU is installed: `qemu-system-x86_64 --version`

**Screenshots have wrong resolution**
- Default resolution is 2560x1600 (Framework 16 native)
- The resolution is stored in `ovmf/OVMF_VARS.fd`, generated from `ovmf_vars.json`.
  An old copy from before that setting existed produces a size mismatch on every
  screenshot. Regenerate it: `rm ovmf/OVMF_VARS.fd && make ovmf/OVMF_VARS.fd`

**Test runner can't find binary**
- Build the app first: `make visual-test` (or `make visual-test-debug` for debug).
  The runner only looks in `target/visual-test/`, not in the normal `target/` dir.

**Screenshot shows "Checking fan status..." instead of the finished screen**
- Press `b` in the scenario to mark EC diagnostics complete

### File Structure

```
tests/visual/
├── Cargo.toml              # Test runner dependencies
├── src/main.rs             # Test runner implementation
├── scenarios/              # Test scenario definitions (YAML)
│   ├── 01_initial_state.yaml
│   ├── 02_input_modules_missing.yaml
│   ├── ...
│   └── 11_outdated_firmware.yaml
├── baselines/              # Expected screenshots (committed to git)
│   ├── initial_no_modules.png
│   └── ...
└── outputs/                # Test run outputs (gitignored)
    ├── *.png               # Actual screenshots
    └── *_diff.png          # Diff images on failure
```
