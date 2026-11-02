# Releasing

Publishing is the owner's decision. Nothing is published except by a pushed tag.

## A dry run
Run the **Release** workflow by hand (Actions, Release, Run workflow): it builds every package as a
workflow artifact and publishes nothing. On Linux it also installs the `.deb` and runs `install.sh`.

## A release
1. Set the version in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and move the
   `[Unreleased]` notes in `CHANGELOG.md` under the new version.
2. Run `cargo run --example licenses` and `cargo xtask ci`, then commit and push.
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`. A tag with a suffix, such as `v0.1.0-rc.1`,
   is a pre-release.

## What it makes
| System | Files |
| --- | --- |
| Linux | `pearl_<version>_amd64.deb`, and a `.tar.gz` with `install.sh`, which needs no administrator rights |
| Windows | `pearl-<version>-windows-x64-setup.exe`, for one user, and a `.zip` |
| macOS | a `.tar.gz` with a universal program, not signed or notarized, so macOS warns before opening it |

`SHA256SUMS.txt` lists them all. Each carries the licences. OpenSSL isn't bundled: on Linux Pearl links to
the system's, which the `.deb` depends on; the Windows and macOS builds use the system's own TLS.

## Not signed
Windows' SmartScreen and macOS' Gatekeeper warn about programs with no signature. Signing needs
certificates that cost money, which is the owner's decision.
