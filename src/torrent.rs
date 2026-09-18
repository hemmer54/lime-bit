use anyhow::{anyhow, Result};
use gosh_dl::{torrent::Metainfo, DownloadEngine, DownloadOptions, DownloadState, EngineConfig};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TorrentInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub progress: f32,
    pub completed_size: u64,
    pub total_size: Option<u64>,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub peers: u32,
    pub connections: u32,
    pub seeders: u32,
    pub eta_seconds: Option<u64>,
    pub location: String,
}

pub struct TorrentEngine {
    pub(crate) engine: Arc<DownloadEngine>,
    ids: HashMap<String, gosh_dl::DownloadId>,
    pending_removals: HashSet<String>,
    default_download_dir: PathBuf,
}

impl TorrentEngine {
    pub async fn new(config: EngineConfig) -> Result<Self> {
        let default_download_dir = config.download_dir.clone();
        let engine = DownloadEngine::new(config).await?;
        Ok(Self {
            engine,
            ids: HashMap::new(),
            pending_removals: HashSet::new(),
            default_download_dir,
        })
    }

    pub async fn add_uri(
        &mut self,
        uri: &str,
        mut options: DownloadOptions,
    ) -> Result<TorrentInfo> {
        let uri = uri.trim();
        if uri.is_empty() {
            return Err(anyhow!("a magnet URI or HTTP(S) URL is required"));
        }
        let id = if let Some(path) = local_torrent_path(uri) {
            let torrent_data = tokio::fs::read(&path).await.map_err(|error| {
                anyhow!("could not read torrent file {}: {error}", path.display())
            })?;
            let metainfo = Metainfo::parse(&torrent_data)?;
            if options.save_dir.is_none() && metainfo.info.is_single_file {
                options.save_dir = Some(
                    self.default_download_dir
                        .join(safe_folder_name(&metainfo.info.name)),
                );
            }
            self.engine
                .add_torrent(&torrent_data, options.clone())
                .await?
        } else if uri.starts_with("magnet:") {
            self.engine.add_magnet(uri, options.clone()).await?
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            self.engine.add_http(uri, options).await?
        } else {
            return Err(anyhow!(
                "unsupported input; use magnet:, http://, https://, or a .torrent file path"
            ));
        };
        self.ids.insert(id.to_gid(), id);
        self.status_for(id)
            .ok_or_else(|| anyhow!("download status was not available after adding"))
    }

    pub async fn pause(&self, id: &str) -> Result<()> {
        self.engine
            .pause(self.lookup(id)?)
            .await
            .map_err(Into::into)
    }
    pub async fn resume(&self, id: &str) -> Result<()> {
        self.engine
            .resume(self.lookup(id)?)
            .await
            .map_err(Into::into)
    }
    pub fn cancel_task(
        &mut self,
        id: &str,
        delete_files: bool,
    ) -> Result<tokio::task::JoinHandle<gosh_dl::Result<()>>> {
        let download_id = self.lookup(id)?;
        // gosh-dl removes the status synchronously at the start of cancel(),
        // but the worker shutdown and file cleanup can finish later. Hide the
        // row during that gap so the next polling snapshot cannot resurrect it.
        self.pending_removals.insert(id.to_string());
        let engine = Arc::clone(&self.engine);
        Ok(tokio::spawn(async move {
            engine.cancel(download_id, delete_files).await
        }))
    }
    pub async fn verify(&self, id: &str) -> Result<String> {
        let report = self.engine.verify(self.lookup(id)?).await?;
        Ok(report.detail)
    }
    pub async fn repair(&self, id: &str) -> Result<String> {
        let report = self.engine.repair(self.lookup(id)?).await?;
        Ok(report.detail)
    }

    pub fn list(&mut self) -> Vec<TorrentInfo> {
        let statuses = self.engine.list();
        let active_ids: HashSet<String> =
            statuses.iter().map(|status| status.id.to_gid()).collect();
        self.pending_removals.retain(|id| active_ids.contains(id));
        self.ids.retain(|id, _| active_ids.contains(id));

        statuses
            .into_iter()
            .filter(|status| !self.pending_removals.contains(&status.id.to_gid()))
            .map(|status| {
                self.ids.insert(status.id.to_gid(), status.id);
                Self::to_info(status)
            })
            .collect()
    }

    fn lookup(&self, id: &str) -> Result<gosh_dl::DownloadId> {
        self.ids
            .get(id)
            .copied()
            .ok_or_else(|| anyhow!("unknown download id: {id}"))
    }

    fn status_for(&self, id: gosh_dl::DownloadId) -> Option<TorrentInfo> {
        self.engine.status(id).map(Self::to_info)
    }

    fn to_info(status: gosh_dl::DownloadStatus) -> TorrentInfo {
        let is_multi_file = status.torrent_info.as_ref().is_some_and(|torrent| {
            torrent.files.len() > 1
                || torrent
                    .files
                    .first()
                    .is_some_and(|file| file.path.components().count() > 1)
        });
        let location = if is_multi_file {
            status.metadata.save_dir.join(&status.metadata.name)
        } else {
            status.metadata.save_dir.clone()
        }
        .to_string_lossy()
        .into_owned();
        TorrentInfo {
            id: status.id.to_gid(),
            name: status.metadata.name,
            state: state_name(&status.state),
            progress: (status.progress.percentage() / 100.0) as f32,
            completed_size: status.progress.completed_size,
            total_size: status.progress.total_size,
            download_speed: status.progress.download_speed,
            upload_speed: status.progress.upload_speed,
            peers: status.progress.peers,
            connections: status.progress.connections,
            seeders: status.progress.seeders,
            eta_seconds: status.progress.eta_seconds,
            location,
        }
    }
}

fn safe_folder_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let sanitized = sanitized.trim().trim_end_matches('.').to_string();
    if sanitized.is_empty() {
        "download".to_string()
    } else {
        sanitized
    }
}

fn local_torrent_path(uri: &str) -> Option<PathBuf> {
    if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("magnet:") {
        return None;
    }
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let path = path
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(path);
    let path = Path::new(path);
    path.extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| extension.eq_ignore_ascii_case("torrent"))
        .map(|_| path.to_path_buf())
}

fn state_name(state: &DownloadState) -> String {
    match state {
        DownloadState::Error { message, .. } => format!("error: {message}"),
        other => format!("{other:?}").to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::local_torrent_path;
    use std::path::Path;

    #[test]
    fn recognizes_torrent_extension_case_insensitively() {
        assert_eq!(
            local_torrent_path("C:/Downloads/example.TORRENT"),
            Some(Path::new("C:/Downloads/example.TORRENT").to_path_buf())
        );
    }

    #[test]
    fn recognizes_file_uri_torrent_path() {
        assert_eq!(
            local_torrent_path("file:///C:/Downloads/example.torrent"),
            Some(Path::new("C:/Downloads/example.torrent").to_path_buf())
        );
    }

    #[test]
    fn does_not_treat_remote_urls_as_local_files() {
        assert_eq!(
            local_torrent_path("https://example.test/file.torrent"),
            None
        );
    }
}
