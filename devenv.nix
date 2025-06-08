{ pkgs, lib, config, inputs, ... }:

{
  packages = with pkgs; [
    systemd # libudev
    # UEFI
    gnumake
    parted
    qemu_kvm
  ];

  languages.rust = {
    enable = true;
    # https://devenv.sh/reference/options/#languagesrustchannel
    channel = "stable";
    targets = [ "x86_64-unknown-uefi" ];
  };
}
