SOURCES=Cargo.toml Cargo.lock src/main.rs

target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi: $(SOURCES)
	mkdir -p esp/efi/boot
	cargo build --target x86_64-unknown-uefi

OVMF_CODE.fd:
	cp /usr/share/OVMF/OVMF_CODE.fd .
OVMF_VARS.fd:
	cp /usr/share/OVMF/OVMF_VARS.fd .

run: OVMF_CODE.fd OVMF_VARS.fd target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi
	cp target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi esp/efi/boot/bootx64.efi
	qemu-system-x86_64 -enable-kvm \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:esp
