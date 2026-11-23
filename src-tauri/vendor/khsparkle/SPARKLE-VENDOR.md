# Vendored Sparkle pin

- Release tag: `2.10.1` (newest stable Sparkle 2 release on 2026-11-23)
- Download URL: https://github.com/sparkle-project/Sparkle/releases/download/2.10.1/Sparkle-2.10.1.tar.xz
- Archive sha256: `1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27`
- Archive size: 16,346,336 bytes
- Framework version: `CFBundleShortVersionString=2.10.1`, `CFBundleVersion=2065` (`Versions/B/Resources/Info.plist`)
- Vendored binary: `vendor/Sparkle.framework/Versions/B/Sparkle`, sha256 `9c78b2046e7637efeb8ca505d63170bc1557651925dbbfd45ac236845732b05a`

The archive ships the framework as `Versions/B` with `Versions/Current -> B` and
no `Versions/A`. The vendored copy is unmodified: an earlier added
`Versions/A -> B` alias was removed because `codesign --verify --deep --strict`
reported "unsealed contents present in the root directory of an embedded
framework". Use `Versions/Current/…` or `Versions/B/…`.

## Layout

`Versions/A` (through the `A -> B` alias) contains: `Headers/` (23 headers), `Modules/`, `PrivateHeaders/`, `Resources/`, `Updater.app/`, `XPCServices/`, `_CodeSignature/`, `Autoupdate`, `Sparkle`.

`Versions/A/Modules` contains `module.modulemap` and `module.private.modulemap` only. There is no `Sparkle.swiftmodule` and no `.swiftmodule` anywhere in the framework: Sparkle 2.10.1 is an Objective-C module imported through the clang module map. Consequently there are no Swiftmodule architecture slices to list.

`Versions/A/XPCServices` contains `Downloader.xpc` and `Installer.xpc`.

`lipo -info vendor/Sparkle.framework/Versions/A/Sparkle`: x86_64 and arm64 (universal).

## Sparkle's own signature (before re-signing)

`codesign -dv --verbose=4 vendor/Sparkle.framework` reported:

```text
Executable=$HOME/Developer/_orchestra/developer-system/sparkle-shim-20261123/vendor/Sparkle.framework/Versions/Current/Sparkle
Identifier=org.sparkle-project.Sparkle
Format=bundle with Mach-O universal (x86_64 arm64)
CodeDirectory v=20500 size=1116 flags=0x10002(adhoc,runtime) hashes=28+3 location=embedded
VersionPlatform=1
VersionMin=786432
VersionSDK=1705216
Hash type=sha256 size=32
CandidateCDHash sha256=8c1a970a736e5fe2a05f33a69e10108013f141d4
CandidateCDHashFull sha256=8c1a970a736e5fe2a05f33a69e10108013f141d4f4c20a680a441b8f8de6604e
Hash choices=sha256
CMSDigest=8c1a970a736e5fe2a05f33a69e10108013f141d4f4c20a680a441b8f8de6604e
CMSDigestType=2
Executable Segment base=0
Executable Segment limit=344064
Executable Segment flags=0x0
Page size=16384
CDHash=8c1a970a736e5fe2a05f33a69e10108013f141d4
Signature=adhoc
Info.plist entries=20
TeamIdentifier=not set
Runtime Version=26.5.0
Sealed Resources version=2 rules=13 files=76
Internal requirements count=0 size=12
Total signatures=1
Chosen signature=1
```

Sparkle's release binary is ad-hoc signed: Team ID is `not set` and there is no `Authority` line. It must be re-signed by the consuming app.

## Key generation tool

`bin/generate_keys` and `bin/sign_update` were present in the archive and copied
to `vendor/bin/` (0755, universal). `generate_keys` was **not** run: it writes
the private key to the Keychain, which the brief forbids. `sign_update` signs
from a key file with `--ed-key-file` — the signature format Sparkle itself
produces — and never touches the Keychain.

`~/.kh-sparkle/ed25519-private.key` on this Mac is a **harness key only**: it
signs the fixture appcast that `harness/run-harness.sh` serves, and nothing else
uses it. **Do not adopt it for an app.** Each app has its own pair, at
`~/.kh-sparkle/<bundle-id>/private.key`, generated once and never regenerated
while an install trusts it — the full rule is in `INTEGRATION.md` §4, which is
the single copy.

Tools kept: `vendor/bin/generate_keys`, `vendor/bin/sign_update` (`BinaryDelta`
and `generate_appcast` were not needed and were left out).
