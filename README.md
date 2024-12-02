# Taildrop Notifier

[![GitHub Actions](https://img.shields.io/endpoint.svg?url=https%3A%2F%2Factions-badge.atrox.dev%2Fnyawox%2Ftaildrop-notifier%2Fbadge%3Fref%3Dmain&style=for-the-badge&labelColor=11111b)](https://actions-badge.atrox.dev/nyawox/taildrop-notifier/goto?ref=main)

A simple Rust ~~script~~ program that allows users to "accept" or "decline" Tailscale file send requests on Linux desktops through user-friendly notifications.

## Quick Start

### Dependencies

- `pipewire`
- `libnotify`
- `cargo`

> **Note:** Root permissions are required to watch and move received files located in `/var/tailscale/files`.

### Installation and Usage

#### Traditional (not tested)

1. Clone the repository:
   ```bash
   git clone https://github.com/nyawox/taildrop-notifier
   ```
2. Build the project:
   ```bash
   cargo build
   ```
3. Run the program:
   ```bash
   sudo ./target/debug/taildrop-notifier -u user-name -p /optional/path/to/receive/files
   ```

#### Nix (Recommended)

1. Run the program with Nix:
   ```bash
   sudo nix run github:nyawox/taildrop-notifier -- -u user-name -p /optional/path/to/receive/files
   ```

2. Alternatively, enable the systemd service using the NixOS module:
   ```nix
   {
     description = "Your NixOS configuration";
     inputs = {
       nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
       taildrop-notifier.url = "github:nyawox/taildrop-notifier";
     };
     outputs = { self, nixpkgs, taildrop-notifier, ... }: {
       nixosConfigurations = {
         yourHost = nixpkgs.lib.nixosSystem {
           system = "x86_64-linux";
           modules = [
             # This is not a complete NixOS configuration; reference your normal configuration here.
             # Import the module
             taildrop-notifier.nixosModules.taildrop-notifier

             ({
               # Enable the service
               services.taildrop-notifier.enable = true;
               services.taildrop-notifier.user = "yourUser";
             })
           ];
         };
       };
     };
   }
   ```

## How It Works

By default, when Tailscale receives a file on Linux desktops, it is automatically sent to:
```
/var/tailscale/files/${username}-uid-${index}
```
without any user prompt.

Since Tailscale runs with root permissions, retrieving the file to user directories requires setting the `--operator` flag to your username, either when starting Tailscale or with the `tailscale set` command.
Once set, you can retrieve the files by running:
```bash
tailscale file get ${download-dir}
```

This program simplifies the process by watching `/var/tailscale/files` for new files. When a file arrives, it prompts the user to either:
- **Accept** (move the file to a user-specified directory with appropriate user permissions), or
- **Decline** (delete the file).

## Credits

Thanks to Borealis theme for the `Kopete_notify.wav`, which I edited to suit my taste.
