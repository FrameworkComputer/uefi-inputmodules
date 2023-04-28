# uefi-experiments

Pre-requisites:

- [Rustup](https://rustup.rs/)

Build:

```
# Build
make

# Copy executable to UEFI System Partition (ESP)
cp target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi /boot/efi

# Make sure to also put a UEFI shell at /boot/efi/EFI/BOOT/BOOTX64.efi
# Or put it somewhere else on the ESP and add a bootloader entry for it
```

Boot to UEFI shell and run:

```
# Switch to partition
fs0:

# Run one of

# GUI. Use the arrow keys to move the logo around
uefi-qmk-hid.efi gui

# Display information about installed GOP protocols
uefi-qmk-hid.efi -b gop

# Display information about installed HII Image decoder protocols
uefi-qmk-hid.efi -b decoder

# Get info about USB devices
uefi-qmk-hid.efi lsusb
uefi-qmk-hid.efi lsusb -v

# Control QMK keyboard
uefi-qmk-hid.efi via --bootloader
uefi-qmk-hid.efi via --rgb-brightness
uefi-qmk-hid.efi via --rgb-brightness 100
uefi-qmk-hid.efi via --rgb-brightness 50
uefi-qmk-hid.efi via --rgb-brightness 0
```

Simulate in QEMU: `make run`
