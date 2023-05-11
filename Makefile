.PHONY: run clean
TARGET:=debug
SOURCES=Cargo.toml $(wildcard src/*.rs) $(wildcard resources/*)

debug: target/x86_64-unknown-uefi/debug/inputmodules.efi
	@ls -lh $<

clean:
	rm -f target/x86_64-unknown-uefi/debug/inputmodules.efi
	rm -f target/x86_64-unknown-uefi/release/inputmodules.efi

qemu: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --features qemu

bmp: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --features bmp

target/x86_64-unknown-uefi/debug/inputmodules.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi

release: target/x86_64-unknown-uefi/release/inputmodules.efi
	@ls -lh $<

target/x86_64-unknown-uefi/release/inputmodules.efi: $(SOURCES)
	cargo build --target x86_64-unknown-uefi --release

clippy:
	cargo clippy --target x86_64-unknown-uefi

# Automatically get OVMF from system path
OVMF_CODE.fd:
	cp /usr/share/OVMF/OVMF_CODE.fd .
OVMF_VARS.fd:
	cp /usr/share/OVMF/OVMF_VARS.fd .

run: OVMF_CODE.fd OVMF_VARS.fd target/x86_64-unknown-uefi/$(TARGET)/inputmodules.efi
	mkdir -p esp/efi/boot
	cp target/x86_64-unknown-uefi/$(TARGET)/inputmodules.efi esp/efi/boot/bootx64.efi
	#cp target/x86_64-unknown-uefi/$(TARGET)/inputmodules.efi esp/inputmodules.efi
	#cp ../firmware-update/res/shell.efi esp/efi/boot/bootx64.efi
	qemu-system-x86_64 \
	  -enable-kvm \
		-M q35 \
		-m 1024 \
		-net none \
		-serial stdio \
		-debugcon file:debug.log -global isa-debugcon.iobase=0x402 \
		-usb \
    -drive if=pflash,format=raw,readonly=on,file=ovmf/OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=ovmf/OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:esp
