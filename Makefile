SOURCES=Cargo.toml Cargo.lock src/main.rs src/raw_hid.rs src/via.rs src/uefi_usb.rs

target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi: $(SOURCES)
	mkdir -p esp/efi/boot
	cargo build --target x86_64-unknown-uefi

clippy:
	cargo clippy --target x86_64-unknown-uefi

OVMF_CODE.fd:
	cp /usr/share/OVMF/OVMF_CODE.fd .
OVMF_VARS.fd:
	cp /usr/share/OVMF/OVMF_VARS.fd .

run: OVMF_CODE.fd OVMF_VARS.fd target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi
	cp target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi esp/efi/boot/bootx64.efi
	qemu-system-x86_64 \
	  -enable-kvm \
		-M q35 \
		-m 1024 \
		-net none \
		-usb -device usb-host,vendorid=0x32ac,productid=0x0012 \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:esp
