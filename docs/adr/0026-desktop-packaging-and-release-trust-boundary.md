# ADR 0026: Desktop packaging and release trust boundary

## Status

Accepted.

## Context

BLCVoice needs reproducible native desktop bundles before production signing or store/release-account work is configured. Packaging is part of the product's compatibility and trust boundary: a bundle that compiles successfully is not automatically signed, notarized, trusted by an operating system, or validated against every supported desktop environment.

The initial packaging branch used one dynamic runner per operating system and granted `contents: write` to the whole workflow. Current upstream guidance creates several avoidable risks:

- Tauri documents that AppImage runtime compatibility is constrained by the glibc version of the build system and recommends an older supported build baseline such as Ubuntu 22.04 or Debian 12.
- GitHub's current hosted-runner matrix makes `macos-latest` arm64, while separate Intel runner labels are available; one macOS runner therefore does not validate both primary desktop architectures.
- Tauri recommends ad-hoc signing when building macOS applications without an Apple-authenticated signing identity, especially for Apple Silicon artifacts.
- GitHub Actions permissions can be scoped per job, so build-only pull-request/manual validation does not require release write permission.

## Decision

BLCVoice desktop packaging uses the following policy:

- Linux x64 release bundles are built on Ubuntu 22.04 and produce `.deb` plus `.AppImage` artifacts.
- Windows x64 release bundles are built on Windows Server 2025 and produce an NSIS installer.
- macOS release bundles are built separately for arm64 (`macos-15`) and x64 (`macos-15-intel`), each producing `.app` and `.dmg` artifacts.
- The declared minimum macOS deployment target remains 10.15.
- macOS validation/draft bundles use the ad-hoc signing identity `-`. This is not Developer ID signing and is not notarization.
- Pull-request and manual validation jobs are build-only, use read-only repository permissions, and upload workflow artifacts without creating a release.
- Tag-triggered `v*` jobs alone receive `contents: write` and may create/update a **draft prerelease**. Draft creation is not production publication authority.
- Updater metadata remains disabled until a separate updater/signing decision is accepted.
- Windows production code signing, macOS Developer ID signing/notarization, credential handling and production publication remain external mandatory human gates.
- Packaging success proves artifact production on the recorded runner only. Compatibility/support and operating-system trust claims require their own evidence.

The `tauri-apps/tauri-action` dependency is pinned to an immutable commit corresponding to the accepted upstream 1.0.0 release rather than a floating tag.

## Alternatives considered

### Build Linux bundles on Ubuntu 24.04

Rejected for release artifacts. It is a valid CI environment, but it unnecessarily raises the likely glibc floor of AppImage output relative to Ubuntu 22.04. Newer Linux images can continue to be used for non-packaging CI.

### Build only `macos-latest`

Rejected. The current GitHub-hosted `macos-latest` runner is arm64, so it does not provide Intel packaging evidence.

### Build a universal macOS binary immediately

Deferred. Separate native arm64 and x64 artifacts are simpler to diagnose and validate. A universal artifact may be introduced later if distribution evidence shows a clear user benefit.

### Leave macOS bundles completely unsigned

Rejected for CI/draft artifacts. Tauri's ad-hoc identity requires no Apple credentials and gives Apple Silicon bundles the code signature expected for Internet-distributed binaries while preserving the explicit distinction from Developer ID signing/notarization.

### Give the whole workflow release-write permission

Rejected. Build-only pull-request/manual jobs require no release mutation authority. Write permission is confined to tag-only draft-release jobs.

## Evidence

Primary upstream references consulted for this decision:

- Tauri GitHub Actions pipeline guide: https://v2.tauri.app/distribute/pipelines/github/
- Tauri AppImage compatibility guidance: https://v2.tauri.app/distribute/appimage/
- Tauri macOS signing guidance: https://v2.tauri.app/distribute/sign/macos/
- Tauri prerequisites: https://v2.tauri.app/start/prerequisites/
- GitHub-hosted runner reference: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- GitHub `GITHUB_TOKEN` workflow-trigger behavior: https://docs.github.com/en/actions/concepts/security/github_token

## Consequences

- Bundle validation costs more CI time because macOS is tested on two architectures, but failures become architecture-specific and support claims are better grounded.
- Linux artifacts favor a broader compatibility baseline instead of the newest Ubuntu image.
- Draft artifacts can be produced autonomously without exposing production signing credentials.
- Production distribution remains intentionally blocked at the signing/notarization/account boundary until the required human-controlled credentials and policies exist.
