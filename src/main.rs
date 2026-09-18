#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod state;
mod torrent;
mod ui;

use crate::state::{AppCommand, AppEvent};
use anyhow::Result;
use gosh_dl::EngineConfig;
use gpui_kit::component::Root;
use gpui_kit::*;
use std::fs;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

#[derive(Clone)]
struct BackendLogWriter {
    tx: mpsc::Sender<AppEvent>,
    buffer: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl BackendLogWriter {
    fn fresh(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl Write for BackendLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = self.buffer.lock().map_err(|_| io::ErrorKind::Other)?;
        buffer.extend_from_slice(bytes);
        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let line = buffer.drain(..=newline).collect::<Vec<_>>();
            let message = String::from_utf8_lossy(&line).trim().to_string();
            if !message.is_empty() {
                let _ = self.tx.try_send(AppEvent::BackendLog(message));
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BackendLogWriter {
    type Writer = BackendLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.fresh()
    }
}

fn main() -> Result<()> {
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>(512);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<AppCommand>(256);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lime_bit=info,gosh_dl=debug"));
    let log_writer = BackendLogWriter {
        tx: event_tx.clone(),
        buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(log_writer)
        .try_init();

    let engine_thread = std::thread::Builder::new()
        .name("lime-bit-engine".into())
        .spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create Tokio runtime");
        runtime.block_on(async move {
            let mut config = EngineConfig::new();
            // Enable the documented BEP 29 uTP path, but retain TCP fallback
            // for peers that do not support uTP or have unreliable UDP.
            config.torrent.utp.enabled = true;
            config.torrent.utp.policy = gosh_dl::config::TransportPolicy::PreferUtp;
            config.torrent.utp.tcp_fallback = true;
            let database_path = config.get_database_path();
            if let Some(parent) = database_path.parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    tracing::warn!("could not create database directory {}: {error}", parent.display());
                }
            }
            config.database_path = Some(database_path);

            let mut engine = match torrent::TorrentEngine::new(config).await {
                Ok(engine) => engine,
                Err(error) => {
                                    tracing::error!("failed to start gosh-dl: {error:#}");
                                    return;
                                }
            };
            // Enforce the app's paused-on-start policy even if gosh-dl restored
            // an older active/Downloading state with a worker already running.
            let startup_pause = engine.engine.pause_all().await;
            if !startup_pause.succeeded.is_empty() {
                tracing::info!(
                    "Paused {} persisted download(s) at startup",
                    startup_pause.succeeded.len()
                );
            }
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            let mut cancel_tasks = Vec::new();

            loop {
                tokio::select! {
                    command = cmd_rx.recv() => match command {
                        Some(AppCommand::Add { uri, options }) => {
                                                    let _ = event_tx.send(AppEvent::Error(format!("Adding {uri}…"))).await;
                                                    send_result(&event_tx, engine.add_uri(&uri, options).await.map(AppEvent::TorrentUpdated)).await;
                                                }
                        Some(AppCommand::Pause(id)) => send_result(&event_tx, engine.pause(&id).await.map(|_| AppEvent::Error(format!("Paused {id}")))).await,
                        Some(AppCommand::Resume(id)) => send_result(&event_tx, engine.resume(&id).await.map(|_| AppEvent::Error(format!("Resumed {id}")))).await,
                        Some(AppCommand::Cancel { id, delete_files }) => {
                            let _ = event_tx.send(AppEvent::TorrentRemoved(id.clone())).await;
                            let _ = event_tx
                                .send(AppEvent::Error(format!("Removing {id}…")))
                                .await;
                            match engine.cancel_task(&id, delete_files) {
                                Ok(task) => {
                                    let tx = event_tx.clone();
                                    cancel_tasks.push(tokio::spawn(async move {
                                        let message = match task.await {
                                            Ok(Ok(())) => format!("Removed {id}"),
                                            Ok(Err(error)) => format!("Remove failed for {id}: {error}"),
                                            Err(error) => format!("Remove task failed for {id}: {error}"),
                                        };
                                        let _ = tx.send(AppEvent::Error(message)).await;
                                    }));
                                }
                                Err(error) => {
                                    let _ = event_tx.send(AppEvent::Error(format!("Remove failed for {id}: {error}"))).await;
                                }
                            }
                        }
                        Some(AppCommand::Verify(id)) => send_result(&event_tx, engine.verify(&id).await.map(|detail| AppEvent::Error(detail))).await,
                        Some(AppCommand::Repair(id)) => send_result(&event_tx, engine.repair(&id).await.map(|detail| AppEvent::Error(detail))).await,
                        Some(AppCommand::OpenLocation(path)) => {
                            let result = open_location(&path)
                                .map(|_| AppEvent::Error(format!("Opened {path}")));
                            send_result(&event_tx, result).await;
                        }
                        Some(AppCommand::SetGlobalLimits { download, upload }) => {
                            let mut config = engine.engine.get_config();
                            config.global_download_limit = download;
                            config.global_upload_limit = upload;
                            let result = engine
                                .engine
                                .set_config(config)
                                .map(|_| AppEvent::Error("Global speed limits applied".into()))
                                .map_err(anyhow::Error::from);
                            send_result(&event_tx, result).await;
                        }
                        Some(AppCommand::PauseAll) => {
                            let result = engine.engine.pause_all().await;
                            send_result(&event_tx, Ok(AppEvent::Error(format!("Paused {} downloads", result.succeeded.len())))).await;
                        }
                        Some(AppCommand::ResumeAll) => {
                            let result = engine.engine.resume_all().await;
                            send_result(&event_tx, Ok(AppEvent::Error(format!("Resumed {} downloads", result.succeeded.len())))).await;
                        }
                        Some(AppCommand::CancelAll { delete_files }) => {
                            while let Some(task) = cancel_tasks.pop() {
                                let _ = task.await;
                            }
                            let result = engine.engine.cancel_all(delete_files).await;
                            // cancel_all removes entries from gosh-dl before its
                            // worker cleanup completes. Reconcile immediately
                            // instead of waiting for the next polling tick.
                            let _ = event_tx.send(AppEvent::TorrentSnapshot(engine.list())).await;
                            let message = if delete_files {
                                format!("Deleted data for {} downloads", result.succeeded.len())
                            } else {
                                format!(
                                    "Cleared torrent cache for {} downloads; files kept",
                                    result.succeeded.len()
                                )
                            };
                            send_result(&event_tx, Ok(AppEvent::Error(message))).await;
                        }
                        Some(AppCommand::Shutdown) | None => {
                            tracing::info!("Shutting down download engine safely");
                            while let Some(task) = cancel_tasks.pop() {
                                let _ = task.await;
                            }
                            let _ = engine.engine.shutdown().await;
                            break;
                        }
                    },
                    _ = interval.tick() => {
                        if event_tx.send(AppEvent::TorrentSnapshot(engine.list())).await.is_err() { return; }
                    }
                }
            }
        });
        })
        .expect("engine thread panicked");

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        cx.spawn(async move |cx| {
            let mut options = gpui_component::TitleBar::window_options();
            options.window_background = gpui::WindowBackgroundAppearance::Transparent;
            options.is_resizable = true;
            options.is_movable = true;
            options.window_min_size = Some(gpui::size(gpui::px(720.0), gpui::px(480.0)));
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| app::LimeBitApp::new(event_rx, cmd_tx, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
    // Do not let the process exit while gosh-dl is still persisting and
    // stopping torrent workers. The window-close callback sends Shutdown.
    let _ = engine_thread.join();
    Ok(())
}

async fn send_result(tx: &mpsc::Sender<AppEvent>, result: anyhow::Result<AppEvent>) {
    let event = result.unwrap_or_else(|error| AppEvent::Error(error.to_string()));
    let _ = tx.send(event).await;
}

fn open_location(path: &str) -> anyhow::Result<()> {
    let path = std::path::Path::new(path);
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        if path.is_file() {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command.spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open")
            .arg(directory)
            .spawn()?;
    }
    Ok(())
}
