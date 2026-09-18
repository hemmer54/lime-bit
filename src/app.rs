use crate::state::{AppCommand, AppEvent};
use crate::torrent::TorrentInfo;
use crate::ui::torrent_row::render_torrent_row;
use gosh_dl::{DownloadOptions, DownloadPriority};
use gpui::{px, ClipboardItem, ExternalPaths, WindowControlArea};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{button::ButtonVariants, InteractiveElementExt};
use gpui_kit::*;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::mpsc::{Receiver, Sender};

type Input = Entity<gpui_component::input::InputState>;

const LIME: u32 = 0xb8ff3c;
const PANEL: u32 = 0x101a12b8;
const SURFACE: u32 = 0x17251bb8;
const TEXT: u32 = 0xecffe1;
const MUTED: u32 = 0xa6bd9c;

#[derive(Clone)]
struct FormInputs {
    save_dir: Input,
    filename: Input,
    user_agent: Input,
    referer: Input,
    headers: Input,
    cookies: Input,
    mirrors: Input,
    max_connections: Input,
    max_download_speed: Input,
    max_upload_speed: Input,
    seed_ratio: Input,
    selected_files: Input,
    priority: Input,
    start_paused: Input,
    sequential: Input,
    seed_after: Input,
}

pub struct LimeBitApp {
    torrents: Vec<TorrentInfo>,
    cmd_tx: Sender<AppCommand>,
    uri_input: Option<Input>,
    form: Option<FormInputs>,
    options_open: bool,
    console_open: bool,
    backend_logs: VecDeque<String>,
    notice: Option<String>,
}

impl LimeBitApp {
    pub fn new(
        mut rx: Receiver<AppEvent>,
        cmd_tx: Sender<AppCommand>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shutdown_tx = cmd_tx.clone();
        cx.on_window_closed(move |_, _| {
            let _ = shutdown_tx.try_send(AppCommand::Shutdown);
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                if !cx.update(|cx| {
                    this.update(cx, |app, cx| {
                        match event {
                            AppEvent::TorrentSnapshot(snapshot) => app.torrents = snapshot,
                            AppEvent::TorrentUpdated(info) => {
                                if let Some(existing) =
                                    app.torrents.iter_mut().find(|t| t.id == info.id)
                                {
                                    *existing = info;
                                } else {
                                    app.torrents.push(info);
                                }
                            }
                            AppEvent::TorrentRemoved(id) => {
                                app.torrents.retain(|torrent| torrent.id != id);
                                // Do not leave an older Add status such as
                                // “Adding…” visible after a removal action.
                                app.notice = None;
                            }
                            AppEvent::Error(message) => app.notice = Some(message),
                            AppEvent::BackendLog(message) => {
                                if app.backend_logs.len() >= 2000 {
                                    app.backend_logs.pop_front();
                                }
                                app.backend_logs.push_back(message);
                            }
                        }
                        cx.notify();
                    })
                    .is_ok()
                }) {
                    break;
                }
            }
        })
        .detach();
        Self {
            torrents: Vec::new(),
            cmd_tx,
            uri_input: None,
            form: None,
            options_open: false,
            console_open: false,
            backend_logs: VecDeque::new(),
            notice: None,
        }
    }
}

impl Render for LimeBitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.uri_input.is_none() {
            self.uri_input = Some(cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx)
                    .placeholder("Magnet URI, HTTP(S) URL, or .torrent path")
            }));
        }
        if self.form.is_none() {
            self.form = Some(FormInputs {
                save_dir: input(window, cx, "Save directory (blank = gosh-dl default)", ""),
                filename: input(window, cx, "Filename", ""),
                user_agent: input(window, cx, "User agent", ""),
                referer: input(window, cx, "Referer", ""),
                headers: input(window, cx, "Headers key:value, comma-separated", ""),
                cookies: input(window, cx, "Cookies, comma-separated", ""),
                mirrors: input(window, cx, "HTTP mirrors, comma-separated", ""),
                max_connections: input(window, cx, "Max connections", "16"),
                max_download_speed: input(
                    window,
                    cx,
                    "Max download bytes/sec (blank = unlimited)",
                    "",
                ),
                max_upload_speed: input(window, cx, "Max upload bytes/sec (blank = unlimited)", ""),
                seed_ratio: input(window, cx, "Seed ratio", "1.0"),
                selected_files: input(
                    window,
                    cx,
                    "Torrent file indices, e.g. 0,2,4 (blank = all)",
                    "",
                ),
                priority: input(
                    window,
                    cx,
                    "Priority: low, normal, high, critical",
                    "normal",
                ),
                start_paused: input(window, cx, "Start paused: true or false", "false"),
                sequential: input(window, cx, "Sequential mode: true or false", "false"),
                seed_after: input(window, cx, "Seed after completion", "true"),
            });
        }
        let input_entity = self.uri_input.as_ref().unwrap().clone();
        let form = self.form.as_ref().unwrap().clone();
        let add_tx = self.cmd_tx.clone();
        let file_tx = self.cmd_tx.clone();
        let file_form = form.clone();
        let drop_tx = self.cmd_tx.clone();
        let drop_form = form.clone();
        let options_button = cx.entity().downgrade();
        let console_button = cx.entity().downgrade();
        let notice = self.notice.clone();
        let console_open = self.console_open;
        let backend_logs = self.backend_logs.iter().cloned().collect::<Vec<_>>();

        let settings = self
            .options_open
            .then(|| settings_panel(&form, window, cx, self.cmd_tx.clone()));
        div()
            .id("lime-bit-root")
            .on_drop(move |paths: &ExternalPaths, _window, app| {
                let options = make_options(&drop_form, app);
                if let Some(path) = paths.paths().iter().find(|path| is_torrent_path(path)) {
                    let tx = drop_tx.clone();
                    let uri = path.to_string_lossy().into_owned();
                    app.background_executor()
                        .spawn(async move {
                            let _ = tx.send(AppCommand::Add { uri, options }).await;
                        })
                        .detach();
                }
            })
            .flex()
            .flex_col()
            .size_full()
            .bg(rgba(PANEL))
            .text_color(rgb(TEXT))
            .border_1()
            .border_color(rgb(LIME))
            .child(title_bar())
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_4()
                    .py_2()
                    .gap_2()
                    .child(
                        gpui_component::input::Input::new(&input_entity)
                            .flex_1()
                            .h_8(),
                    )
                    .child(button("CONSOLE", MUTED, move |cx| {
                        let _ = console_button.update(cx, |app, cx| {
                            app.console_open = !app.console_open;
                            cx.notify();
                        });
                    }))
                    .child(button("OPTIONS", MUTED, move |cx| {
                        let _ = options_button.update(cx, |app, cx| {
                            app.options_open = !app.options_open;
                            cx.notify();
                        });
                    }))
                    .child(button("OPEN .TORRENT", MUTED, move |cx| {
                        let tx = file_tx.clone();
                        let options = make_options(&file_form, cx);
                        cx.background_executor()
                            .spawn(async move {
                                if let Some(file) = rfd::AsyncFileDialog::new()
                                    .set_title("Open torrent file")
                                    .add_filter("BitTorrent metainfo", &["torrent"])
                                    .pick_file()
                                    .await
                                {
                                    let _ = tx
                                        .send(AppCommand::Add {
                                            uri: file.path().to_string_lossy().into_owned(),
                                            options,
                                        })
                                        .await;
                                }
                            })
                            .detach();
                    }))
                    .child(button("ADD DOWNLOAD", LIME, move |cx| {
                        let uri = input_entity.read(cx).value().to_string();
                        if uri.trim().is_empty() {
                            return;
                        }
                        let options = make_options(&form, cx);
                        let tx = add_tx.clone();
                        cx.background_executor()
                            .spawn(async move {
                                let _ = tx.send(AppCommand::Add { uri, options }).await;
                            })
                            .detach();
                    })),
            )
            .children(settings)
            .child(toolbar(&self.torrents, self.cmd_tx.clone()))
            .children(notice.map(|message| {
                div()
                    .px_4()
                    .py_2()
                    .text_sm()
                    .text_color(rgb(LIME))
                    .child(message)
            }))
            .child(
                div().flex_1().flex_col().p_4().gap_2().children(
                    self.torrents
                        .iter()
                        .map(|torrent| render_torrent_row(torrent, self.cmd_tx.clone())),
                ),
            )
            .children(console_open.then(|| backend_console(&backend_logs)))
    }
}

fn input(
    window: &mut Window,
    cx: &mut Context<LimeBitApp>,
    placeholder: &'static str,
    default: &'static str,
) -> Input {
    cx.new(|cx| {
        gpui_component::input::InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(default)
    })
}

fn title_bar() -> impl IntoElement {
    div()
        .id("lime-bit-titlebar")
        .flex()
        .items_center()
        .h(px(34.0))
        .w_full()
        .bg(rgba(SURFACE))
        .border_b_1()
        .border_color(rgb(LIME))
        .child(
            div()
                .flex()
                .items_center()
                .id("lime-bit-title-drag")
                .flex_1()
                .h_full()
                .px_4()
                .on_double_click(|_, window, _| window.zoom_window())
                .window_control_area(WindowControlArea::Drag)
                .child(
                    div()
                        .child("lime-bit")
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(LIME)),
                ),
        )
        .child(window_control("MIN"))
        .child(window_control("MAX"))
        .child(window_control("X"))
}

fn window_control(label: &'static str) -> impl IntoElement {
    div()
        .id(format!("lime-bit-window-{label}"))
        .flex()
        .items_center()
        .justify_center()
        .w(px(44.0))
        .h(px(34.0))
        .bg(rgba(0x203a26ff))
        .border_1()
        .border_color(rgb(LIME))
        .text_sm()
        .text_color(rgb(TEXT))
        .hover(|style| style.bg(rgba(0x38552fff)).text_color(rgb(LIME)))
        .child(match label {
            "MIN" => "—",
            "MAX" => "⛶",
            "X" => "×",
            _ => "?",
        })
        .on_click(move |_, window, _| match label {
            "MIN" => window.minimize_window(),
            "MAX" => window.zoom_window(),
            "X" => window.remove_window(),
            _ => {}
        })
}

fn backend_console(logs: &[String]) -> impl IntoElement {
    let log_text = logs.join("\n");
    div()
        .flex()
        .flex_col()
        .mx_2()
        .mb_2()
        .p_2()
        .w_full()
        .h_48()
        .bg(rgba(0x08100be6))
        .border_1()
        .border_color(rgb(0x38552f))
        .rounded_md()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .child("BACKEND CONSOLE")
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(LIME)),
                )
                .child(button("COPY", MUTED, move |cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(log_text.clone()));
                })),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .text_xs()
                .text_color(rgb(MUTED))
                .children(logs.iter().map(|line| div().child(line.clone()))),
        )
}

fn toolbar(torrents: &[TorrentInfo], tx: Sender<AppCommand>) -> impl IntoElement {
    let active = torrents
        .iter()
        .filter(|torrent| {
            matches!(
                torrent.state.as_str(),
                "connecting" | "downloading" | "seeding"
            )
        })
        .count();
    let waiting = torrents
        .iter()
        .filter(|torrent| torrent.state == "queued")
        .count();
    let speed: u64 = torrents.iter().map(|torrent| torrent.download_speed).sum();
    let uploaded: u64 = torrents.iter().map(|torrent| torrent.upload_speed).sum();
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_4()
        .py_2()
        .gap_2()
        .bg(rgba(SURFACE))
        .border_b_1()
        .border_color(rgb(LIME))
        .child(
            div()
                .child(format!(
                    "{} active · {} queued · ↓ {:.2} MB/s · ↑ {:.2} MB/s",
                    active,
                    waiting,
                    speed as f64 / 1_000_000.0,
                    uploaded as f64 / 1_000_000.0
                ))
                .text_sm()
                .text_color(rgb(MUTED)),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(global_button(
                    "PAUSE ALL",
                    0xf2c879,
                    tx.clone(),
                    AppCommand::PauseAll,
                ))
                .child(global_button(
                    "RESUME ALL",
                    LIME,
                    tx.clone(),
                    AppCommand::ResumeAll,
                ))
                .child(global_button(
                    "CLEAR CACHE · KEEP FILES",
                    0xf2c879,
                    tx.clone(),
                    AppCommand::CancelAll {
                        delete_files: false,
                    },
                ))
                .child(global_button(
                    "DELETE DATA",
                    0xff7b8b,
                    tx,
                    AppCommand::CancelAll { delete_files: true },
                )),
        )
}

fn global_button(
    label: &'static str,
    color: u32,
    tx: Sender<AppCommand>,
    command: AppCommand,
) -> impl IntoElement {
    button(label, color, move |cx| {
        let tx = tx.clone();
        let command = command.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = tx.send(command).await;
            })
            .detach();
    })
}

fn settings_panel(
    form: &FormInputs,
    window: &mut Window,
    cx: &mut Context<LimeBitApp>,
    tx: Sender<AppCommand>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .px_4()
        .py_2()
        .gap_2()
        .bg(rgba(0x0b1510d9))
        .border_1()
        .border_color(rgb(0x38552f))
        .rounded_lg()
        .child(
            div()
                .child("DOWNLOAD OPTIONS")
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(LIME)),
        )
        .child(row(
            "Storage",
            vec![
                field("Save directory", &form.save_dir),
                field("Filename", &form.filename),
            ],
        ))
        .child(row(
            "Network",
            vec![
                field("Max connections", &form.max_connections),
                field("Download limit (bytes/s)", &form.max_download_speed),
                field("Upload limit (bytes/s)", &form.max_upload_speed),
            ],
        ))
        .child(row(
            "Torrent",
            vec![
                field("Seed ratio", &form.seed_ratio),
                field("Selected files", &form.selected_files),
                choice_field(
                    "Priority",
                    &form.priority,
                    &["low", "normal", "high", "critical"],
                    window,
                    cx,
                ),
            ],
        ))
        .child(row(
            "Flags",
            vec![
                choice_field(
                    "Start paused",
                    &form.start_paused,
                    &["false", "true"],
                    window,
                    cx,
                ),
                choice_field(
                    "Sequential",
                    &form.sequential,
                    &["false", "true"],
                    window,
                    cx,
                ),
                choice_field(
                    "Seed after completion",
                    &form.seed_after,
                    &["true", "false"],
                    window,
                    cx,
                ),
            ],
        ))
        .child(div().flex().justify_end().child(global_button(
            "APPLY GLOBAL SPEED LIMITS",
            MUTED,
            tx,
            make_global_limits_command(form, cx),
        )))
        .child(row(
            "HTTP",
            vec![
                field("User agent", &form.user_agent),
                field("Referer", &form.referer),
                field("Mirrors", &form.mirrors),
            ],
        ))
        .child(row(
            "Auth",
            vec![
                field("Headers", &form.headers),
                field("Cookies", &form.cookies),
            ],
        ))
}

fn choice_field(
    label: &'static str,
    input: &Input,
    choices: &'static [&'static str],
    _window: &mut Window,
    cx: &mut Context<LimeBitApp>,
) -> AnyElement {
    let current = input.read(cx).value().to_string();
    let mut dropdown = gpui_component::button::DropdownButton::new(label)
        .button(
            gpui_component::button::Button::new(format!("{label}-button"))
                .label(current)
                .compact()
                .outline()
                .secondary(),
        )
        .outline();
    let input_for_menu = input.clone();
    dropdown = dropdown.dropdown_menu(move |menu, _window, _cx| {
        choices.iter().fold(menu, |menu, choice| {
            let input = input_for_menu.clone();
            let value = (*choice).to_string();
            menu.item(gpui_component::menu::PopupMenuItem::new(*choice).on_click(
                move |_event, window, cx| {
                    let value = value.clone();
                    let _ = input.update(cx, |state, cx| {
                        state.set_value(value, window, cx);
                    });
                },
            ))
        })
    });
    div()
        .flex_1()
        .flex_col()
        .gap_1()
        .child(div().child(label).text_xs().text_color(rgb(MUTED)))
        .child(dropdown)
        .into_any_element()
}

fn field(label: &'static str, input: &Input) -> AnyElement {
    div()
        .flex_1()
        .flex_col()
        .gap_1()
        .child(div().child(label).text_xs().text_color(rgb(MUTED)))
        .child(
            gpui_component::input::Input::new(input)
                .h_8()
                .bg(rgba(0x101a12e6))
                .border_1()
                .border_color(rgb(0x38552f))
                .rounded_md()
                .px_2(),
        )
        .into_any_element()
}

fn row(label: &'static str, inputs: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .p_2()
        .bg(rgba(0x13201899))
        .rounded_md()
        .child(div().w_20().child(label).text_sm().text_color(rgb(MUTED)))
        .children(inputs)
}

fn button<F>(label: &'static str, color: u32, handler: F) -> impl IntoElement
where
    F: Fn(&mut App) + 'static,
{
    let mut button = gpui_component::button::Button::new(label)
        .label(label)
        .compact()
        .outline();
    button = if color == LIME {
        button.success()
    } else if color == 0xff7b8b {
        button.danger()
    } else if color == MUTED {
        button.secondary()
    } else {
        button.warning()
    };
    button.on_click(move |_event, _window, cx| handler(cx))
}

fn make_options(form: &FormInputs, cx: &mut App) -> DownloadOptions {
    let mut options = DownloadOptions::new();
    let value = |input: &Input| input.read(cx).value().to_string();
    let text = |input: &Input| {
        let value = value(input);
        (!value.trim().is_empty()).then_some(value)
    };
    options.start_paused = value(&form.start_paused)
        .trim()
        .eq_ignore_ascii_case("true");
    options.sequential = Some(value(&form.sequential).trim().eq_ignore_ascii_case("true"));
    options.save_dir = text(&form.save_dir).map(PathBuf::from);
    options.filename = text(&form.filename);
    options.user_agent = text(&form.user_agent);
    options.referer = text(&form.referer);
    options.max_connections = value(&form.max_connections).trim().parse().ok();
    options.max_download_speed = value(&form.max_download_speed).trim().parse().ok();
    options.max_upload_speed = value(&form.max_upload_speed).trim().parse().ok();
    options.seed_ratio = if value(&form.seed_after).trim().eq_ignore_ascii_case("false") {
        Some(0.0)
    } else {
        value(&form.seed_ratio).trim().parse().ok()
    };
    options.selected_files = parse_list(&value(&form.selected_files));
    options.priority = DownloadPriority::from_str(value(&form.priority).trim()).unwrap_or_default();
    options.mirrors = split_list(&value(&form.mirrors));
    options.cookies = nonempty_list(&value(&form.cookies));
    options.headers = value(&form.headers)
        .split(',')
        .filter_map(|entry| {
            let (key, value) = entry.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    options
}

fn make_global_limits_command(form: &FormInputs, cx: &mut App) -> AppCommand {
    let value = |input: &Input| input.read(cx).value().to_string();
    let limit = |input: &Input| {
        let value = value(input);
        value.trim().parse::<u64>().ok().filter(|limit| *limit > 0)
    };
    AppCommand::SetGlobalLimits {
        download: limit(&form.max_download_speed),
        upload: limit(&form.max_upload_speed),
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
fn nonempty_list(value: &str) -> Option<Vec<String>> {
    let values = split_list(value);
    (!values.is_empty()).then_some(values)
}
fn parse_list(value: &str) -> Option<Vec<usize>> {
    let values: Vec<_> = value
        .split(',')
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    (!values.is_empty()).then_some(values)
}

fn is_torrent_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"))
}
