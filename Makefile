SOURCES=Cargo.toml $(wildcard src/*.rs)

target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi

release: target/x86_64-unknown-uefi/release/uefi-qmk-hid.efi

target/x86_64-unknown-uefi/release/uefi-qmk-hid.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --release

clippy:
	cargo clippy --target x86_64-unknown-uefi

OVMF_CODE.fd:
	cp /usr/share/OVMF/OVMF_CODE.fd .
OVMF_VARS.fd:
	cp /usr/share/OVMF/OVMF_VARS.fd .

run: OVMF_CODE.fd OVMF_VARS.fd target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi
	mkdir -p esp/efi/boot
	cp target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi esp/uefi-qmk-hid.efi
	cp ../firmware-update/res/shell.efi esp/efi/boot/bootx64.efi
	qemu-system-x86_64 \
	  -enable-kvm \
		-M q35 \
		-m 1024 \
		-net none \
		-usb -device usb-host,vendorid=0x32ac,productid=0x0012 \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:esp
