# rauri

A minimal AUR helper for Arch Linux.

## Install

```bash
cargo install rauri
```

## Usage

```bash
rauri -Q <package>     # Search
rauri -S <package>     # Install
rauri -Syu             # Update
rauri -R <package>     # Remove
rauri -L               # List installed
rauri -P <path>        # Set download directory
```

## Config

`~/.config/rauri/config.toml`
