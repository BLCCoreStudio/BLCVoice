# ADR 0027: Store transcription history as text-only local SQLite data

- Status: Accepted
- Date: 2026-09-05

## Context

ADR 0001 includes basic local history in the first usable BLCVoice milestone. `ARCHITECTURE.md` assigns history and migrations to the storage boundary, requires local ownership of dictation data, and keeps raw-audio retention off by default.

History is sensitive user content. The persistence mechanism must therefore remain local, survive normal application restarts and crashes, support explicit deletion, evolve through migrations, and stay outside the desktop UI layer.

The current desktop bootstrap already resolves Tauri's app-specific data directory. Tauri 2 documents the app data directory as the suggested location for application data, namespaced by the configured bundle identifier.

For the embedded database layer, `rusqlite` 0.40.2 documents its `bundled` feature as the appropriate default for applications that control their own SQLite database and need to avoid depending on a missing or old system SQLite. That release bundles SQLite 3.53.2. SQLite documents transactional atomic commit across crashes/power loss. SQLite also documented and fixed a rare WAL-reset corruption race in 2026 affecting older releases under concurrent WAL writers/checkpoints; BLCVoice does not need multi-writer WAL for its initial history workload.

Comparable local-first transcription projects commonly retain transcript text in local SQLite while discarding microphone audio. Echo, for example, stores transcription text in a local SQLite database and discards audio after local transcription. Muesly likewise uses local SQLite for transcript data.

## Decision

BLCVoice will add a dedicated Rust storage boundary for basic transcription history with these rules:

1. **Storage location** — the desktop application supplies a database path below Tauri's app-specific data directory. Storage code receives a path; it does not depend on Tauri.
2. **Database** — use SQLite through `rusqlite` with the `bundled` feature so released desktop bundles carry one reviewed SQLite version consistently across Linux, Windows and macOS.
3. **Concurrency** — the initial store owns one process-local connection behind the storage service. Use SQLite's rollback-journal transaction model rather than enabling WAL. WAL can be reconsidered only with evidence that concurrent readers/writers materially need it.
4. **Durability** — schema changes and history writes are transactional. Database/schema failures are typed and surfaced; BLCVoice must never silently reset, replace or delete a history database after corruption or migration failure.
5. **Schema evolution** — maintain an explicit integer schema version and forward migrations owned by the storage crate. Opening a database with a newer unsupported schema fails closed.
6. **Data retained** — persist transcript text and minimal provenance/delivery metadata needed to explain an entry: timestamp, invocation source, recognition engine/model identifiers, detected language when known, insertion backend when applicable, and a truthful delivery state.
7. **Delivery semantics** — history must distinguish transcription from insertion outcome. A backend receipt that cannot verify target-document mutation is recorded as backend-submitted/unverified, never as semantically delivered.
8. **Audio retention** — raw microphone audio, processed PCM and VAD buffers are never written to history storage by default.
9. **Retention control** — history is local and user-removable. The storage contract provides deterministic bounded listing and explicit deletion; broader retention automation requires a later product decision.
10. **Layering** — SQL, migrations and retention semantics stay in the storage crate. Tauri commands/application orchestration may call the storage service; JavaScript/UI may only request and render typed history results.

The initial schema will not add full-text search, cloud sync, encryption-at-rest claims, semantic search or background retention policies. Those features require separate evidence and, where material, a superseding ADR.

## Alternatives considered

### JSON or JSONL files

This avoids an embedded database dependency but makes transactional delete/update, migrations, corruption handling, bounded ordered queries and cross-platform atomic replacement behavior application-owned. That is unnecessary persistence complexity for sensitive user data.

### SQLite with a system library

This reduces binary size on some platforms but makes runtime behavior depend on whichever SQLite version the target system provides. BLCVoice already produces self-contained cross-platform desktop packages; one reviewed bundled SQLite version is more predictable.

### SQLite WAL mode

WAL is useful for workloads with meaningful concurrent read/write pressure. Initial BLCVoice history is a low-volume, single-process, single-writer workload, so WAL adds files, checkpoint behavior and concurrency surface without demonstrated benefit. The 2026 WAL-reset bug is additional evidence against enabling WAL casually, although the selected bundled SQLite version contains the fix.

### SQLCipher by default

Database encryption can be valuable but introduces key-management, recovery, platform crypto and distribution decisions that are not solved merely by linking SQLCipher. BLCVoice will not imply encryption-at-rest without a complete key-management design. Local OS account/storage protections remain the current boundary.

## Consequences

### Positive

- History remains local and text-only by default.
- SQL and schema policy stay outside the UI and Tauri-specific layers.
- Transactional persistence and explicit migrations reduce corruption/data-loss risk compared with ad-hoc files.
- Bundled SQLite makes behavior more reproducible across release platforms.
- Delivery metadata can remain truthful when insertion is submitted but semantically unverifiable.

### Negative

- The desktop binary gains SQLite/rusqlite code and build time.
- Transcript text remains sensitive data at rest and is not application-encrypted by this decision.
- A migration policy becomes a compatibility responsibility for future releases.

## Validation requirements

Before history is called implemented:

- storage migration/open/reopen tests;
- append/list ordering and metadata round-trip tests;
- bounded-query and explicit-delete tests;
- corruption/newer-schema failure tests without destructive reset;
- Rust formatting, tests, Clippy and RustSec;
- Linux, Windows and macOS compile/package validation with the bundled SQLite dependency;
- desktop wiring tests proving history persistence cannot turn an insertion failure into a delivery success or cause raw audio retention.

## References

- Tauri 2 path API, `appDataDir`: https://v2.tauri.app/reference/javascript/api/namespacepath/#appdatadir
- rusqlite 0.40.2 documentation: https://docs.rs/crate/rusqlite/0.40.2
- SQLite atomic commit: https://www.sqlite.org/atomiccommit.html
- SQLite WAL documentation and 2026 WAL-reset fix: https://www.sqlite.org/wal.html
- Echo local transcription/history design: https://github.com/BBQHQ/echo
- Muesly local-first storage design: https://github.com/afonsojramos/muesly
