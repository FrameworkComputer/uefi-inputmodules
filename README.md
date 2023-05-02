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

## Running

Framework Laptop 16 BIOS will ship with this app embedded.

To run a modified version, build it, as below, or download a release binary.

It can be started on the shell, just by starting the executable.
For debugging, providing any commandline arguments displays the inputmodule
status on the commandline:

```
# Launch GUI
Shell> fs0:inputmodules.efi

# Show debug
Shell> fs0:inputmodules.efi debug
Input Deck State: On
Touchpad present: true
Positions:
  Pos 0: KeyboardA
  Pos 1: Disconnected
  Pos 2: Disconnected
  Pos 3: KeyboardB
  Pos 4: Disconnected
```

To test in QEMU build the QEMU version and run it.
QEMU has a separate version because the screen resolution is much smaller.

Pre-reqs:

- Need to put `OVMF_CODE.fd` and `OVMF_VARS.fd` in the root folder (if it doesn't work automatically.

```sh
make qemu
make run
```

## Building

Pre-requisites:

- [Rustup](https://rustup.rs/)

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
