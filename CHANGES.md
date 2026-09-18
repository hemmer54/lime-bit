# Changes

## Unreleased — 2026-09-18

### Torrent reliability

- Track the upstream `gosh-dl` GitHub `main` branch so the client receives
  the latest BitTorrent request-loop and cancellation fixes.
- Remove the wrapper's competing five-second cancellation timeout; the UI now
  waits for gosh-dl's bounded stop operation and reports the actual result
  instead of incorrectly warning that removal is taking too long.

- Enable preferred uTP transport with TCP fallback for `gosh-dl 0.6.3`; UDP
  tracker announces remain supported by the engine tracker client.
- Add tracing output with `RUST_LOG`/`RUST_TRACE` support so tracker, peer,
  handshake, choking, and piece-request failures are visible during debugging.
- Surface real torrent telemetry in rows: connected peers, seeders, and ETA in
  addition to transfer rates and progress.
- Keep cancellation responsive by removing the row immediately and running the
  engine cancellation/worker shutdown in a background Tokio task with a bounded
  status report.
- Prevent polling snapshots from resurrecting downloads while removal is still
  cleaning up, and reconcile Remove All immediately.
- Add a COPY button to the backend console and retain TCP fallback for uTP
  transport compatibility.
- Clear stale Add notices when removing an individual torrent and default the
  in-app gosh-dl tracing filter to debug for peer/request diagnosis.
- Replace the native Windows titlebar with a compact GPUI titlebar with
  explicit visible `—`, `⛶`, and `×` window controls, and tune the main
  layout for denser, more consistent scaling.
- Keep persisted torrents paused at startup so reopening the app never starts
  network activity unexpectedly.
- Enable BEP 29 uTP with TCP fallback; gosh-dl’s tracker client continues to
  support BEP 15 UDP tracker announces automatically.
- Add a window-close shutdown handshake that persists/stops active torrents
  before the process exits.
- Keep persisted downloads paused on startup to prevent unexpected network
  activity; existing verified files remain available when Resume is pressed.
- Simplify the client titlebar to `lime-bit` and add a stable GPUI drag region.
- Make CLEAR CACHE · KEEP FILES remove torrent/session metadata while
  preserving downloaded folders, and add separate destructive DELETE DATA.
- Add OPEN LOCATION to each torrent row using the engine-backed save path.
- Put single-file torrents in their own sanitized folder under Downloads;
  multi-file torrents retain gosh-dl’s torrent-name directory layout.
- Move the backend console below the torrent list, expand retention to 2,000
  lines, add global download/upload limit application, and add a seed-after-
  completion opt-out.

### Options and UI

- Hide the Windows console window for normal GUI launches.
- Add a toggleable in-app backend console fed by `tracing-subscriber`, while
  preserving the aggregate speed toolbar and torrent row capacity.

- Replace raw priority, start-paused, and sequential text fields with GPUI
  component dropdown controls.
- Keep numeric, path, HTTP, authentication, mirror, and selected-file options
  as labeled themed inputs because those values are free-form or numeric.
- Preserve the dark, translucent Lime-Bit theme and GPUI component buttons.
- Add a native `.torrent` file picker with a visible BitTorrent metainfo file
  type filter.
- Accept `.torrent` files dragged directly onto the Lime-Bit window, while
  ignoring unrelated dropped files.

### Validation

- Added extension-filtered picker and operating-system file-drop handling.

- `cargo fmt -- --check` passed.
- `cargo check` passed.
- `cargo test` passed; the crate currently contains no local test cases.

### Known limitation

A torrent can report peers while transferring zero bytes when those peers are
choking the client, do not advertise needed pieces, or the swarm has no usable
seed. The new connection/seeder/ETA telemetry and tracing output distinguish
that swarm condition from a worker failure. The next diagnostic step for a
specific torrent is to run with `RUST_LOG=gosh_dl=debug` and inspect the peer
handshake, bitfield, unchoke, and block-request messages.
