# Lime-Bit

Lime-Bit is a Rust desktop BitTorrent/download client built with GPUI and
[`gosh-dl`](https://github.com/goshitsarch-eng/gosh-dl). It is currently an
early-stage beta focused on torrent transfers, persistence, diagnostics,
and a compact dark green/lime interface.

> **Beta software:** test with non-critical data. The torrent engine currently
> verifies existing pieces when a torrent worker starts, which can make resume
> take time for large downloads.

## Features

- Magnet links, `.torrent` files, HTTP(S) downloads, file picker, and drag/drop.
- TCP peer transport with preferred BEP 29 uTP and TCP fallback.
- UDP and HTTP tracker announces, DHT, PEX, LPD, and webseed support through
  `gosh-dl`.
- Real progress, peer, seeder, connection, speed, and ETA telemetry.
- Persistent download state with conservative paused-on-start behavior.
- Safe window-close shutdown that persists and stops active workers.
- `OPEN LOCATION` for the actual torrent output folder.
- `CLEAR CACHE · KEEP FILES` to remove torrent/session metadata without deleting
  downloaded files.
- `DELETE DATA` for explicit destructive cleanup.
- Per-download limits plus global upload/download limits in Settings.
- Optional seeding after completion, configurable per download.
- Backend console with up to 2,000 tracing lines and clipboard copying.
- Custom compact `lime-bit` titlebar and resizable window layout.

## Requirements

- Rust 1.85 or newer.
- Windows is the primary tested desktop target.
- Network access for trackers, DHT, peers, and webseeds.

The project follows the upstream `gosh-dl` Git branch from `Cargo.toml`; the
resolved revision is recorded in `Cargo.lock` for reproducible builds.

## Build and run

Debug build:

```powershell
cargo run
```

Optimized build:

```powershell
cargo build --release
```

The release executable is created at:

```text
target/release/lime-bit.exe
```

The `target/` directory is build output and is intentionally not committed.

## Beta testing

To share a Windows beta, send only the release executable from
`target/release/lime-bit.exe`. Rust and Cargo are not required on the tester's
machine.

Ask testers to report:

- Windows version.
- Whether the custom titlebar, drag region, and window controls work.
- Whether torrents reopen paused after restarting the app.
- Whether transfers use webseeds, uTP, or TCP peers.
- The copied backend console output for any failure.

Windows SmartScreen may warn because development builds are not code-signed.

## Diagnostics

The in-app backend console defaults to a useful gosh-dl debug filter. For
additional logging, run from PowerShell with:

```powershell
$env:RUST_LOG="lime_bit=info,gosh_dl=debug"
cargo run
```

Useful messages include tracker announcements, peer handshakes, uTP/TCP
fallback, DHT/PEX discovery, choking, piece requests, verification, and
webseed writes.

A tracker timeout is normally isolated; other trackers, DHT, PEX, webseeds, and
existing peers may continue working. A torrent can also have peers with zero
throughput when those peers are choking the client or do not have needed
pieces.

## Data and cleanup

The default download directory is the user's Downloads directory. Single-file
`.torrent` downloads are placed in their own sanitized torrent-named folder;
multi-file torrents use the engine's torrent-name folder layout.

- **Remove:** removes the torrent record while preserving files.
- **CLEAR CACHE · KEEP FILES:** removes torrent/session metadata and preserves
  downloaded files.
- **DELETE DATA:** removes the torrent record and downloaded files.

Existing torrent data is never deleted by ordinary startup or shutdown.
Persisted torrents reopen paused and require an explicit Resume action.

## Development checks

Before submitting changes:

```powershell
cargo fmt -- --check
cargo check
cargo test
```

Project conventions and verified architecture notes are maintained in
[`project_context.mdl`](project_context.mdl). The unreleased user-facing log
is maintained in [`CHANGES.md`](CHANGES.md).

## Current limitation

`gosh-dl 0.6.3` performs a full existing-piece verification when a torrent
worker starts. This is correct for data integrity but can make resuming large
torrents slow. Transmission-style incremental resume verification would need
support in the upstream engine or a maintained fork.
