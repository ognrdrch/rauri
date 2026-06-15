# rauri

A minimal AUR helper written in Rust.

## Install
crates.io Method
```bash
cargo install rauri
```
AUR Method
```bash
git clone https://aur.archlinux.org/rauri.git
cd rauri
makepkg -si
```
## Usage

```bash
Commands
 rauri -Q <package>     # Search AUR & Official Packages
 rauri -S <package>     # Install AUR & Official Packages
 rauri -S               # Update AUR packages only
 rauri -Syu             # Update whole system (official + AUR)
 rauri -R <package>     # Remove AUR & Official Packages
 rauri -L               # List installed AUR Packages
Options    
 rauri -P <path>        # Set download directory
 rauri -C               # Clear AUR Downloads 
```
