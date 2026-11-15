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
 rauri -Q <package>        # Search AUR & Official Packages
 rauri -QA <package>       # Search, showing all results
 rauri -S <package>        # Install AUR & Official Packages
 rauri -Syu                # Update official packages (pacman -Syu)
 rauri -Syu --update-aur   # Update official packages, then AUR packages
 rauri -Syyu               # Force-refresh databases, then update (also: -Syu --refresh)
 rauri --update-aur        # Update AUR packages only
 rauri -R <package>...     # Remove AUR & Official Packages
 rauri -Rs <package>       # ...and their unneeded dependencies (like pacman -Rs)
 rauri -Rns <package>      # ...and also their config backups (like pacman -Rns)
 rauri -L                  # List installed AUR Packages
 rauri -LA                 # List all installed packages, outdated first (also: -L --all)
 rauri -E <package>...     # Exclude packages from -Syu / -Syyu / --update-aur
 rauri -E                  # Show excluded packages
 rauri -I <package>...     # Include excluded packages in updates again
 rauri <AUR_URL>           # Install from an AUR git link
Options    
 rauri -P <path>           # Set download directory
 rauri -C                  # Clear AUR Downloads 
 rauri --fix-sig           # Fix keyring / missing PGP keys without asking
```

## Removing packages

`-R` takes the same modifiers as `pacman -R`, in any order: `s` (recursive), `n` (nosave),
`c` (cascade) and `u` (unneeded), e.g. `-Rs`, `-Rns`, `-Rsc`. The long forms `--recursive`,
`--nosave`, `--cascade` and `--unneeded` work with `--remove`. Several packages can be removed
at once.

pacman shows everything it is about to remove and asks before doing it. Afterwards rauri stops
tracking every package that was removed, including AUR dependencies taken along by `-s` or
`-c`, and deletes their build folders. A folder is kept while another package built from it
(split packages) is still installed.

## Changing the download path

`rauri -P <path>` sets where AUR packages are cloned and built. When you change it, rauri:

1. Offers to move the package folders (folders containing a `PKGBUILD`) from the old path
   to the new one. Folders whose name already exists in the new path are skipped, and other
   files are left alone. The old directory is removed if it ends up empty.
2. Saves the new path to the config. Tracked packages are stored by name, so nothing else
   needs updating.
3. Looks for AUR packages in the new path that are installed but weren't installed with
   rauri (built by hand or by another AUR helper) and offers to track them, so
   `rauri -S` and `rauri -Syu` keep them updated. The same check runs during first-time setup.

## Error handling

rauri watches pacman and makepkg output for common failures. When it recognises one,
it explains the problem and asks before fixing it, then retries the original command.

| Problem | What rauri does |
|---|---|
| Package signature errors (outdated keyring) | If your synced databases already have a newer keyring, offers to install just that (`pacman -S --needed archlinux-keyring`, no system upgrade). Otherwise offers `pacman -Sy --needed archlinux-keyring` followed by `pacman -Su` (skipped when the failed command was already `-Syu`, since the retry upgrades). If that fails, offers to re-initialise the keyring with `pacman-key --init` and `--populate archlinux` |
| AUR source signed by an unknown PGP key | Offers to import the key with `gpg --recv-keys`, falling back to `keyserver.ubuntu.com` |
| AUR package needs other AUR packages | Looks the dependencies up in the AUR and offers to build and install them first (as dependencies), then builds the package again. Works recursively and stops on dependency cycles |
| pacman database locked | If no pacman process is running, offers to remove the leftover `/var/lib/pacman/db.lck` |
| Package downloads fail | Offers to force-refresh the databases and upgrade (`pacman -Syyu`) |
| Disk full, file conflicts, package conflicts, checksum mismatches | Prints a hint on how to resolve it. These are never fixed automatically |

### `--fix-sig`

Pass `--fix-sig` to apply the signature fixes without being asked, e.g. after a failed install:

```bash
rauri -S --fix-sig <package>
rauri -Syu --fix-sig
```

For AUR packages it also imports the PGP keys listed in the package's `.SRCINFO` before
building. makepkg still only accepts signatures from those exact keys.

### Notes

- pacman and makepkg run with `LC_ALL=C` so rauri can recognise their error messages,
  so their output is always in English.
- makepkg runs inside a pseudo-terminal (via `script` from util-linux) so its coloured
  output is kept.
- Prompts default to yes. Without a terminal (e.g. in scripts) they answer no.
