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

# Run
uefi-qmk-hid.efi

# Should display some info about USB devices
```
