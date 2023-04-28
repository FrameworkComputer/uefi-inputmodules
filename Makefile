.PHONY=run
TARGET:=debug
SOURCES=Cargo.toml $(wildcard src/*.rs)

debug: target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi
	@ls -lh $<

qemu: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --features qemu

bmp: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --features bmp

target/x86_64-unknown-uefi/debug/uefi-qmk-hid.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi

release: target/x86_64-unknown-uefi/release/uefi-qmk-hid.efi
	@ls -lh $<

target/x86_64-unknown-uefi/release/uefi-qmk-hid.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --release

clippy:
	cargo clippy --target x86_64-unknown-uefi

OVMF_CODE.fd:
	cp /usr/share/OVMF/OVMF_CODE.fd .
OVMF_VARS.fd:
	cp /usr/share/OVMF/OVMF_VARS.fd .

#	-usb -device usb-host,vendorid=0x32ac,productid=0x0012
run: OVMF_CODE.fd OVMF_VARS.fd target/x86_64-unknown-uefi/$(TARGET)/uefi-qmk-hid.efi
	mkdir -p esp/efi/boot
	cp ~/clone/uefi-rs/./target/x86_64-unknown-uefi/debug/examples/shell_params.efi esp/
	cp target/x86_64-unknown-uefi/$(TARGET)/uefi-qmk-hid.efi esp/uefi-qmk-hid.efi
	cp ../firmware-update/res/shell.efi esp/efi/boot/bootx64.efi
	# Needs sudo in order to pass USB device through
	sudo qemu-system-x86_64 \
	  -enable-kvm \
		-M q35 \
		-m 1024 \
		-net none \
		-serial stdio \
		-debugcon file:debug.log -global isa-debugcon.iobase=0x402 \
		-usb \
		-device usb-ehci,id=ehci \
		-device usb-host,bus=ehci.0,hostbus=3,hostport=2 \
    -drive if=pflash,format=raw,readonly=on,file=ovmf/OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=ovmf/OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:esp
