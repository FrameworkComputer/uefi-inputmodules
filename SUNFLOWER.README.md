# Sunflower

# Framework Laptop 12 - UEFI Debug App

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

Once in the GUI you can use ESC to quit.

## Building

Pre-requisites:

- [Rustup](https://rustup.rs/) (MSRV: 1.61)

Build:

```
# Build debug version
> make
-rwxrwxr-x 2 zoid zoid 695K May  2 11:48 target/x86_64-unknown-uefi/debug/inputmodules.efi

# Build release version
> make release
-rwxrwxr-x 2 zoid zoid 426K May  2 11:49 target/x86_64-unknown-uefi/release/inputmodules.efi

# Copy executable to UEFI System Partition (ESP)
cp target/x86_64-unknown-uefi/debug/inputmodules.efi /boot/efi

# Make sure to also put a UEFI shell at /boot/efi/EFI/BOOT/BOOTX64.efi
# Or put it somewhere else on the ESP and add a bootloader entry for it
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

###### Running in native Framework Laptop 12 resolution

QEMU/OVMF has the capability to display different resolutions.
During early startup, press ESC repeatedly until you enter the setup menu.
There navigate to "Device Manager" -> "OVMF Platform Configuration" and change
the preferred resolution to 1920x1200.

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
- Framework Laptop 12 User Guide
  - TODO
- Framework Laptop 12 Limitations
  - TODO

## Debug prints

- Presence detection, see qemu special keys
- G-Sensor
    - Lid G-Sensor xyz Data
    - Base G-Sensor xyz Data
    - Calculated Lid Angle
    - Tablet Mode
- ALS Data

## QEMU Special Keys

Controls:

- A, S, D, F, G: AC/Battery states (No Battery, 5%, 5% charging, 20%, 20% charging)
- C: Toggle fan (Cooler)
- J: Toggle Audio Jack board
- P: Toggle Power button board
- M: Toggle Cam/GSensor/Mic board (Not implemented yet)
- 1, 2, 3, 4, 5: Input Deck (See below)

Presence/Status detection

- Input deck (1-5)
  - Chassis switch
  - BOARD_ID connected
  - Can talk to touch controller over I2C
  - Can talk to KB controller over I2C
  - Cases
    - 1: Chassis switch open (don't care about the rest)
    - 2: Chassis closed, board ID not present
    - 3: Chassis closed, board ID present, touch I2C ok,     KB I2C not ok
    - 4: Chassis closed, board ID present, touch I2C not ok, KB I2C ok
    - 5: Chassis closed, board ID present,(touch I2C ok,     KB I2C ok) - All good
- Audio Board (present, not present)
- Power Button (Unable to tell, no board ID)
- Camera (No board ID, can tell via G-Sensor I2C)
  - I2C G-Sensor

## Release Notes

### Version 0.4.8

- Unsigned binary SHA256: TBD
- Authenticode SHA256:    TBD
- Unsigned Size:          TBDKB
- Changes
    - Move error message box up a bit to avoid it being cut off

### Version 0.4.7

- Unsigned binary SHA256: 7ca4fe16046442d9a8fe9210fc4c824e804557d85fe1fb7afe0f408fb7bd3f2b
- Authenticode SHA256:    9b532c0dbcd5c9a766f7d386d8b2180d36d173e3a1d506c6a00e95dd1ae75558
- Unsigned Size:          401KB
- Changes
    - Skip resolution update if already native
    - Add current resolution to ? debug info
    - Add commandline flag `inputmodules.efi gop` to print GOP modes for debugging

### Version 0.4.6

- File size: 400K
- Unsigned binary SHA256: 6de7d05c796e8897778d5721c5f5bafeb582aa5104512ff207f84c725e232d17
- Authenticode SHA256: f079af4198516169b98bb750d66d3fb01dadb6548e934f8dc569fa199c92b88e
- Changes
    - Use the highest available resolution for external monitor

### Version 0.4.5

- File size: 396K
- Changes
    - Don't require specific SMBIOS entries
    - Update framework_lib to 32ab4f7a2b9fb55f9a53d4b60c17624029ba017b

### Version 0.4.4

- File size: 419K
- Changes
    - Highlight RAM flap in orange if any issues with pogo pins
    - If external screen resolution larger than sunflower built-in, clear extra pixels to white

### Version 0.4.3

- File size: 421K
- Working
  - Does update chassis switch live
  - Does update battery level and AC live
  - Does detect if fan is present (hotplug detection not needed)
  - Does immediately exit, if everything is okay
  - Can show g-sensor data live
  - Can exit and skip with ESC
  - Short power button press will exit app and continue boot
  - Long power button press will reboot
  - Audio board status is updated live on hot plug
  - Power button board status is updated live on hot plug (previously not at all)
  - Touchpad status is updated live on hot plug
- Changes in this release
  - Don't depend on new host command, can already work with 2.04 EC
- Known issues
  - Does not distinguish between keyboard and touchpad status
  - Does not detect G-Sensor/Camera board
  - Exits after a few minutes (both QEMU or hardware)

### Version 0.4.2

- File size: 535K
- Working
  - Does update chassis switch live
  - Does update battery level and AC live
  - Does detect if fan is present (hotplug detection not needed)
  - Does immediately exit, if everything is okay
  - Can show g-sensor data live
  - Can exit and skip with ESC
  - Short power button press will exit app and continue boot
  - Long power button press will reboot
- Fixed in this release
  - Audio board status is updated live on hot plug
  - Power button board status is updated live on hot plug (previously not at all)
  - Touchpad status is updated live on hot plug
- Update graphic design
  - Change to cleaner design
  - Highlight bad components instead of border
- Known issues
  - Does not distinguish between keyboard and touchpad status
  - Does not detect G-Sensor/Camera board
  - Exits after a few minutes (both QEMU or hardware)

### Version 0.4.1

- Update graphic design
  - Hide G-Sensor debug bubbles and lid angle in release build
  - Show open system
  - Highlight problem areas with red boxes
- Known issues - urgent
  - Graphic Design not final
  - Does not update touchpad/keyboard status live
  - Add note to tell user to shut down if non-hotpluggable components are missing
- Known issues - lower priority
  - Does not detect audio daughterboard (Fixed with EC PR #1440 2.04)
  - Does not distinguish between keyboard and touchpad status (To fix with EC 2.05 and newer inputmodule app)
  - Does not detect powerbutton daughterboard (To fix with EC 2.05 and newer inputmodule app)
  - Does not detect G-Sensor/Camera board (Fixed with EC PR #1440 2.04 and newer inputmodule app)

### Version 0.4.0

First version of sunflower UEFI input module app.

- File size: 280K
- Working
  - Does update chassis switch live
  - Does update battery level and AC live
  - Does detect if fan is present (hotplug detection not needed)
  - Does immediately exit, if everything is okay
  - Can show g-sensor data live
  - Can exit and skip with ESC
  - Short power button press will exit app and continue boot
  - Long power button press will reboot
- Known issues - urgent
  - Graphic Design not final
  - Does not update touchpad/keyboard status live
  - Add note to tell user to shut down if non-hotpluggable components are missing
- Known issues - lower priority
  - Does not detect audio daughterboard (Fixed with EC PR #1440 2.04)
  - Does not distinguish between keyboard and touchpad status (To fix with EC 2.05 and newer inputmodule app)
  - Does not detect powerbutton daughterboard (To fix with EC 2.05 and newer inputmodule app)
  - Does not detect G-Sensor/Camera board (Fixed with EC PR #1440 2.04 and newer inputmodule app)
- Notes
  - Does not update battery connection status live (will never work, battery is not hotplug!)
