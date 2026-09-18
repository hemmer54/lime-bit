use crate::state::AppCommand;
use crate::torrent::TorrentInfo;
use gpui_component::button::ButtonVariants;
use gpui_kit::*;
use tokio::sync::mpsc::Sender;

const LIME: u32 = 0xb8ff3c;
const SURFACE: u32 = 0x1b2b20b8;
const MUTED: u32 = 0xa6bd9c;

pub fn render_torrent_row(torrent: &TorrentInfo, cmd_tx: Sender<AppCommand>) -> impl IntoElement {
    let pause_id = torrent.id.clone();
    let resume_id = torrent.id.clone();
    let cancel_id = torrent.id.clone();
    let verify_id = torrent.id.clone();
    let repair_id = torrent.id.clone();
    let location = torrent.location.clone();
    let location_button_id = location.clone();
    let pause_button_id = pause_id.clone();
    let resume_button_id = resume_id.clone();
    let verify_button_id = verify_id.clone();
    let repair_button_id = repair_id.clone();
    let cancel_button_id = cancel_id.clone();
    let pause_tx = cmd_tx.clone();
    let resume_tx = cmd_tx.clone();
    let cancel_tx = cmd_tx.clone();
    let verify_tx = cmd_tx.clone();
    let repair_tx = cmd_tx.clone();
    let location_tx = cmd_tx;
    let percent = (torrent.progress * 100.0).clamp(0.0, 100.0);
    let size = torrent
        .total_size
        .map(|total| format!("{} / {}", bytes(torrent.completed_size), bytes(total)))
        .unwrap_or_else(|| format!("{} downloaded", bytes(torrent.completed_size)));

    let controls = div()
        .flex()
        .gap_1()
        .child(action("PAUSE", &pause_button_id, move |cx| {
            let tx = pause_tx.clone();
            let id = pause_id.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx.send(AppCommand::Pause(id)).await;
                })
                .detach();
        }))
        .child(action("RESUME", &resume_button_id, move |cx| {
            let tx = resume_tx.clone();
            let id = resume_id.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx.send(AppCommand::Resume(id)).await;
                })
                .detach();
        }))
        .child(action("VERIFY", &verify_button_id, move |cx| {
            let tx = verify_tx.clone();
            let id = verify_id.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx.send(AppCommand::Verify(id)).await;
                })
                .detach();
        }))
        .child(action("REPAIR", &repair_button_id, move |cx| {
            let tx = repair_tx.clone();
            let id = repair_id.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx.send(AppCommand::Repair(id)).await;
                })
                .detach();
        }))
        .child(action("OPEN LOCATION", &location_button_id, move |cx| {
            let tx = location_tx.clone();
            let location = location.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx.send(AppCommand::OpenLocation(location)).await;
                })
                .detach();
        }))
        .child(action("REMOVE", &cancel_button_id, move |cx| {
            let tx = cancel_tx.clone();
            let id = cancel_id.clone();
            cx.background_executor()
                .spawn(async move {
                    let _ = tx
                        .send(AppCommand::Cancel {
                            id,
                            delete_files: false,
                        })
                        .await;
                })
                .detach();
        }));

    div()
        .flex()
        .flex_col()
        .p_3()
        .mb_2()
        .gap_2()
        .bg(rgba(SURFACE))
        .border_1()
        .border_color(rgb(LIME))
        .rounded_md()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex_col()
                        .child(
                            div()
                                .child(torrent.name.clone())
                                .font_weight(FontWeight::BOLD),
                        )
                        .child(
                            div()
                                .child(format!(
                                    "{} · {} · {} peers · {} connected · {} seeders{}",
                                    torrent.state,
                                    size,
                                    torrent.peers,
                                    torrent.connections,
                                    torrent.seeders,
                                    torrent
                                        .eta_seconds
                                        .map(|eta| format!(" · ETA {}s", eta))
                                        .unwrap_or_default()
                                ))
                                .text_sm()
                                .text_color(rgb(MUTED)),
                        ),
                )
                .child(
                    div()
                        .child(format!("{percent:.1}%"))
                        .text_lg()
                        .text_color(rgb(LIME)),
                ),
        )
        .child(
            div().h_2().w_full().bg(rgb(0x253b28)).rounded_sm().child(
                div()
                    .h_full()
                    .w(relative(percent / 100.0))
                    .bg(rgb(LIME))
                    .rounded_sm(),
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .child(format!(
                            "↓ {:.2} MB/s   ↑ {:.2} MB/s",
                            torrent.download_speed as f64 / 1_000_000.0,
                            torrent.upload_speed as f64 / 1_000_000.0
                        ))
                        .text_sm()
                        .text_color(rgb(MUTED)),
                )
                .child(controls),
        )
}

fn action<F>(label: &'static str, id: &str, handler: F) -> impl IntoElement
where
    F: Fn(&mut App) + 'static,
{
    let mut button = gpui_component::button::Button::new(format!("{label}-{id}"))
        .label(label)
        .compact()
        .outline();
    button = match label {
        "REMOVE" => button.danger(),
        "PAUSE" => button.warning(),
        "RESUME" => button.success(),
        "VERIFY" => button.info(),
        "OPEN LOCATION" => button.secondary(),
        _ => button.secondary(),
    };
    button.on_click(move |_event, _window, cx| handler(cx))
}

fn bytes(value: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = value as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
