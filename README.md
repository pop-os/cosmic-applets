# COSMIC Applets Development

This repository is a Rust workspace for COSMIC panel applets and panel buttons.

## Install Build Dependencies

On Debian or Ubuntu

```bash
sudo apt build-dep .
```

## Getting Started


A [justfile](./justfile) is included by default with common recipes used by other COSMIC projects. Install from [casey/just][just]

- `just` builds the application with the default `just build-release` recipe
- `sudo just install` installs the project into the system
- `just vendor` creates a vendored tarball
- `just build-vendored` compiles with vendored dependencies from that tarball
- `just check` runs clippy on the project to check for linter warnings
- `just check-json` can be used by IDEs that support LSP


## Reload The Current Applets

After installing a new build, restart the panel so COSMIC relaunches the applets:

```bash
systemctl --user restart cosmic-session.target
pkill cosmic-panel
```

You can also restore the packaged version by reinstalling the distro package:

```bash
sudo apt install --reinstall cosmic-applets
pkill cosmic-panel
```
