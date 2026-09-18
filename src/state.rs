use crate::torrent::TorrentInfo;
use gosh_dl::DownloadOptions;

#[derive(Debug, Clone)]
pub enum AppEvent {
    TorrentUpdated(TorrentInfo),
    TorrentSnapshot(Vec<TorrentInfo>),
    TorrentRemoved(String),
    Error(String),
    BackendLog(String),
}

#[derive(Debug, Clone)]
pub enum AppCommand {
    Add {
        uri: String,
        options: DownloadOptions,
    },
    Pause(String),
    Resume(String),
    Cancel {
        id: String,
        delete_files: bool,
    },
    Verify(String),
    Repair(String),
    OpenLocation(String),
    SetGlobalLimits {
        download: Option<u64>,
        upload: Option<u64>,
    },
    PauseAll,
    ResumeAll,
    CancelAll {
        delete_files: bool,
    },
    Shutdown,
}
