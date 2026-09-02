# Security Policy

BLCVoice handles sensitive desktop capabilities such as microphone input, local transcription data, clipboard/text insertion and, in future releases, optional integrations with external tools. Security and privacy reports are therefore treated as first-class project issues.

## Supported versions

BLCVoice is currently pre-alpha. No version is considered production-ready or security-supported yet.

Once public releases begin, this section will list supported release lines explicitly.

## Reporting a vulnerability

Please **do not open a public GitHub issue** for suspected vulnerabilities that could expose user data, credentials, microphone access, local files, clipboard contents, integration permissions or arbitrary command execution.

Use GitHub's private vulnerability reporting for this repository when available.

A useful report should include:

- affected version or commit,
- operating system and relevant desktop environment,
- reproduction steps,
- expected and observed behavior,
- impact assessment,
- proof-of-concept details when safe to share privately.

Please avoid collecting or sharing unrelated personal data while reproducing an issue.

## Security principles

The project intends to follow these defaults:

- local-first processing where practical,
- no raw-audio retention by default,
- least-privilege integration permissions,
- no plaintext storage of provider secrets,
- explicit trust boundaries between the UI, core, platform adapters and external integrations,
- minimal GitHub Actions permissions,
- dependency and code scanning in CI,
- signed/verified release artifacts when distributable builds begin.

These are design requirements, not claims that the current pre-alpha repository already implements every control.
