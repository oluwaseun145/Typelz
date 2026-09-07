use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use directories::ProjectDirs;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Required files for the Parakeet TDT v3 ONNX model (INT8 variants — the
/// CPU-recommended quantization from the source repo, ~640 MB total vs ~2.4 GB
/// for the FP16 encoder, which stores its weights in a separate 2.26 GB
/// external data file).
const REQUIRED_FILES: &[&str] = &[
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "vocab.txt",
];

/// HuggingFace repo containing the ONNX files.
const HF_REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";

/// Pinned SHA-256 checksums (lowercase hex) for each required file, taken from
/// the repo's object listing on 2026-09-07. A download whose hash differs is
/// deleted and reported as an error instead of being cached (F-24).
const REQUIRED_CHECKSUMS: &[(&str, &str)] = &[
    (
        "encoder-model.int8.onnx",
        "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
    ),
    (
        "decoder_joint-model.int8.onnx",
        "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
    ),
    (
        "vocab.txt",
        "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
    ),
];

/// Pinned checksum for a required file, if one is known.
fn expected_checksum(file_name: &str) -> Option<&'static str> {
    REQUIRED_CHECKSUMS
        .iter()
        .find(|(file, _)| *file == file_name)
        .map(|(_, hash)| *hash)
}

/// Builds the HuggingFace download URL for a required file.
fn download_url(file_name: &str) -> String {
    format!("https://huggingface.co/{HF_REPO}/resolve/main/{file_name}")
}

/// Computes the lowercase hex SHA-256 of a file, streaming in 64 KiB chunks.
fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_of(hasher.finalize()))
}

fn hex_of(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Verifies a freshly downloaded file against its pinned checksum, deleting
/// the file when the hash does not match.
fn verify_download(path: &Path, file_name: &str) -> Result<(), String> {
    let Some(expected) = expected_checksum(file_name) else {
        return Ok(());
    };
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_file(path);
        return Err(format!(
            "Checksum mismatch for {file_name}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

/// Payload for the `model-status` Tauri event emitted while models download.
#[derive(Serialize, Clone)]
pub struct ModelStatusEvent {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<DownloadProgress>,
}

/// Byte progress for a single file being downloaded.
#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub downloaded: u64,
    /// Total bytes when the server advertised a content length, else 0.
    pub total: u64,
}

/// Status of the model cache directory.
#[derive(Serialize, Clone, Debug)]
pub enum ModelStatus {
    Ready { path: String },
    // Serialized status for the frontend contract; download progress is
    // reported via `model-status` events while a download is in flight.
    #[allow(dead_code)]
    Downloading { downloaded_bytes: u64, total_bytes: u64 },
    NotDownloaded { missing_files: Vec<String> },
    Partial { present: Vec<String>, missing: Vec<String> },
    Error { message: String },
}

/// Cache directory for the Parakeet model.
pub struct ModelCache {
    dir: PathBuf,
}

impl ModelCache {
    /// Returns the OS-specific cache directory for Typelz models.
    pub fn new() -> Result<Self, String> {
        let dirs = ProjectDirs::from("com", "Typelz", "Typelz")
            .ok_or_else(|| "Failed to determine cache directory".to_string())?;
        let dir = dirs.cache_dir().join("models").join("parukeet-tdt-v3-onnx");
        Ok(Self { dir })
    }

    /// Builds a cache view over an explicit directory (used by path-based ensure).
    pub fn for_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Returns the path to the model cache directory.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Checks which required files are present in the cache.
    pub fn check(&self) -> (Vec<String>, Vec<String>) {
        let mut present = Vec::new();
        let mut missing = Vec::new();
        for &file in REQUIRED_FILES {
            if self.dir.join(file).exists() {
                present.push(file.to_string());
            } else {
                missing.push(file.to_string());
            }
        }
        (present, missing)
    }

    /// Returns the current status of the model cache.
    pub fn status(&self) -> ModelStatus {
        let (present, missing) = self.check();
        if missing.is_empty() {
            ModelStatus::Ready {
                path: self.dir.to_string_lossy().to_string(),
            }
        } else if present.is_empty() {
            ModelStatus::NotDownloaded { missing_files: missing }
        } else {
            ModelStatus::Partial { present, missing }
        }
    }

    /// Ensures all required model files exist, reporting download progress
    /// through `on_progress(file, downloaded_bytes, total_bytes_or_none)`.
    pub fn ensure_with_progress(
        &self,
        on_progress: Option<&dyn Fn(&str, u64, Option<u64>)>,
    ) -> Result<ModelStatus, String> {
        let status = self.status();
        if let ModelStatus::Ready { .. } = status {
            return Ok(status);
        }

        let missing = match status {
            ModelStatus::NotDownloaded { missing_files } => missing_files,
            ModelStatus::Partial { present: _, missing } => missing,
            _ => return Ok(status),
        };

        if !missing.is_empty() {
            download_model_sync(self.path(), on_progress)?;
        }

        Ok(self.status())
    }
}

/// Ensures all required model files exist at the given path, downloading any
/// that are missing using a timed-out HTTP client.
pub fn ensure_on_path(
    cache_dir: &Path,
    on_progress: Option<&dyn Fn(&str, u64, Option<u64>)>,
) -> Result<ModelStatus, String> {
    ModelCache::for_dir(cache_dir.to_path_buf()).ensure_with_progress(on_progress)
}

/// Synchronous model download with a 5-minute timeout per request.
/// Files already present in `cache_dir` are skipped, so an interrupted
/// download resumes rather than restarting. Each file is streamed to a
/// `.partial` temp file and renamed into place so a failure never leaves a
/// truncated file that looks like a valid cache entry.
pub fn download_model_sync(
    cache_dir: &Path,
    on_progress: Option<&dyn Fn(&str, u64, Option<u64>)>,
) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {e}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    for &file_name in REQUIRED_FILES {
        let dest = cache_dir.join(file_name);
        if dest.exists() {
            continue;
        }

        let url = download_url(file_name);
        eprintln!("Downloading {file_name}...");
        if let Some(p) = on_progress {
            p(file_name, 0, None);
        }

        let mut resp = client
            .get(&url)
            .send()
            .map_err(|e| format!("Failed to download {file_name}: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Failed to download {file_name}: HTTP {}", resp.status()));
        }
        let total = resp.content_length();

        let tmp = cache_dir.join(format!("{file_name}.partial"));
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| format!("Failed to create {}: {e}", tmp.display()))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut written: u64 = 0;
        let mut last_report: u64 = 0;
        loop {
            let n = resp
                .read(&mut buf)
                .map_err(|e| format!("Failed to download {file_name}: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
            written += n as u64;
            // Throttle progress events so multi-GB downloads don't flood the
            // event loop: at most one per 8 MiB plus a final report.
            if let Some(p) = on_progress {
                if written.saturating_sub(last_report) >= 8 * 1024 * 1024 {
                    last_report = written;
                    p(file_name, written, total);
                }
            }
        }
        if let Some(p) = on_progress {
            p(file_name, written, total);
        }

        verify_download(&tmp, file_name)?;

        std::fs::rename(&tmp, &dest)
            .map_err(|e| format!("Failed to move {file_name} into place: {e}"))?;
        eprintln!("Downloaded {file_name} ({written} bytes)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_required_files_not_empty() {
        assert!(!REQUIRED_FILES.is_empty());
        assert_eq!(REQUIRED_FILES.len(), 3);
    }

    #[test]
    fn test_download_url_matches_pinned_repo() {
        // Replaces test_hf_repo_format (F-26): validates the constructed URL
        // instead of asserting on the repo string literal.
        assert_eq!(
            download_url("vocab.txt"),
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt",
        );
        for &file in REQUIRED_FILES {
            let url = download_url(file);
            assert!(url.starts_with("https://huggingface.co/"));
            assert!(url.ends_with(file));
        }
    }

    #[test]
    fn test_every_required_file_has_pinned_checksum() {
        for &file in REQUIRED_FILES {
            assert!(
                expected_checksum(file).is_some(),
                "no pinned checksum for {file}"
            );
        }
    }

    #[test]
    fn test_sha256_file_known_value() {
        let path = std::env::temp_dir().join(format!("typelz-sha-known-{}", std::process::id()));
        std::fs::write(&path, b"abc").expect("write temp file");
        assert_eq!(
            sha256_file(&path).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_verify_download_rejects_mismatch() {
        let path = std::env::temp_dir().join(format!("typelz-sha-mismatch-{}", std::process::id()));
        std::fs::write(&path, b"definitely not the model").expect("write temp file");
        let err = verify_download(&path, "vocab.txt").expect_err("must reject wrong hash");
        assert!(err.contains("Checksum mismatch"));
        assert!(!path.exists(), "mismatched file must be deleted, not cached");
    }

    #[test]
    fn test_for_dir_status_transitions() {
        let dir = std::env::temp_dir().join(format!("typelz-model-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let cache = ModelCache::for_dir(dir.clone());

        assert!(matches!(cache.status(), ModelStatus::NotDownloaded { ref missing_files } if missing_files.len() == 3));

        std::fs::write(dir.join("vocab.txt"), b"test").expect("write vocab");
        assert!(matches!(cache.status(), ModelStatus::Partial { ref missing, .. } if missing.len() == 2));

        for f in ["encoder-model.int8.onnx", "decoder_joint-model.int8.onnx"] {
            std::fs::write(dir.join(f), b"test").expect("write model file");
        }
        assert!(matches!(cache.status(), ModelStatus::Ready { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_download_skips_existing_files() {
        let dir = std::env::temp_dir().join(format!("typelz-model-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // Every required file present: download is a no-op and must not touch
        // the network.
        for f in REQUIRED_FILES {
            std::fs::File::create(dir.join(f)).expect("create file").write_all(b"test").expect("write");
        }
        let called = std::sync::atomic::AtomicUsize::new(0);
        let result = download_model_sync(
            &dir,
            Some(&|_file: &str, _done: u64, _total: Option<u64>| {
                called.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }),
        );
        assert!(result.is_ok());
        assert_eq!(called.load(std::sync::atomic::Ordering::Relaxed), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
