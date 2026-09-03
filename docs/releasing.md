# Desktop release process

BLCVoice uses `.github/workflows/bundle.yml` to build platform-native desktop bundles with Rust 1.98.0. Packaging follows ADR 0026 and does not turn a successful build into a compatibility, signing or production-release claim.

## Pull-request and manual bundle validation

Changes that can affect the desktop bundle trigger build-only validation. The same build-only jobs can be started with `workflow_dispatch`.

Current validation outputs:

- Linux x64 in a Debian 12 container: `.deb` and `.AppImage`
- Windows x64 on Windows Server 2025: NSIS installer
- macOS arm64 on macOS 15: `.app` and `.dmg`
- macOS x64 on macOS 15 Intel: `.app` and `.dmg`

Validation jobs use read-only repository permissions and upload generated bundles only as workflow artifacts. They do not create a GitHub Release.

Debian 12 is intentionally used as the Linux packaging baseline. Tauri identifies Debian 12 as a suitable WebKitGTK 4.1/AppImage compatibility baseline, while BLCVoice's CPAL 0.18.2 native PipeWire backend requires PipeWire 0.3.53 or newer. Debian 12 provides PipeWire 0.3.65; Ubuntu 22.04's 0.3.48 development headers are too old for that backend. Normal compile/test CI may use newer Linux images independently.

## Tagged draft releases

A push of a version tag matching `v*` runs the same platform/architecture packaging policy with release permission scoped only to the tag-only draft-release jobs. The workflow creates or updates a **draft prerelease** and uploads the produced bundles.

Creating a draft prerelease is not permission to publish a production release. A maintainer must inspect the artifacts, compatibility evidence and external signing state before publication.

Updater metadata remains disabled until a separate updater/signing policy is accepted.

## Signing boundary

The repository intentionally does not contain production signing identities or credentials.

- Windows NSIS bundles remain unsigned until an Authenticode signing policy and credentials are explicitly configured.
- macOS validation and draft bundles use Tauri's ad-hoc signing identity (`-`) so Apple Silicon artifacts have a code signature without requiring Apple credentials.
- Ad-hoc signing is **not** Developer ID signing and does not imply notarization or Gatekeeper trust.
- Production Developer ID signing/notarization, Windows production code signing, certificate/account operations and credential handling remain mandatory human stop gates.

A successful bundle job proves that the package can be produced on the declared build environment. It does not prove trust-store acceptance, semantic text-insertion compatibility, or support on every target machine/application.

Never downgrade these distinctions in release notes. Runtime insertion compatibility and operating-system signing are separate release gates.
