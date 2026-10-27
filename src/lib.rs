//! Zed's sandboxed adapter installs the native client for this extension version.
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use zed_extension_api::{self as zed, Result};

const REPOSITORY: &str = "Prajwal-ak-0/zed-sendline";
const VERSION: &str = env!("CARGO_PKG_VERSION");

struct SendlineExtension;

impl SendlineExtension {
    fn binary(&self, id: &zed::LanguageServerId, worktree: &zed::Worktree) -> Result<String> {
        if let Some(binary) =
            zed::settings::LspSettings::for_worktree(id.as_ref(), worktree)?.binary
            && let Some(path) = binary.path
        {
            return Ok(path);
        }
        if let Some(path) = worktree.which("sendline") {
            return Ok(path);
        }
        let target = match zed::current_platform() {
            (zed::Os::Mac, zed::Architecture::Aarch64) => "aarch64-apple-darwin",
            (zed::Os::Mac, zed::Architecture::X8664) => "x86_64-apple-darwin",
            (zed::Os::Linux, zed::Architecture::Aarch64) => "aarch64-unknown-linux-gnu",
            (zed::Os::Linux, zed::Architecture::X8664) => "x86_64-unknown-linux-gnu",
            _ => return Err("Sendline currently supports macOS and Linux.".into()),
        };
        let directory = format!("sendline-v{VERSION}-{target}");
        let binary = format!("{directory}/sendline");
        if !Path::new(&binary).is_file() {
            zed::set_language_server_installation_status(
                id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            let release = zed::github_release_by_tag_name(REPOSITORY, &format!("v{VERSION}"))
                .map_err(|error| format!("Sendline v{VERSION} is not available: {error}"))?;
            let asset_name = format!("sendline-{VERSION}-{target}.tar.gz");
            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == asset_name)
                .ok_or_else(|| format!("Missing release asset: {asset_name}"))?;
            let checksum_name = format!("sendline-{VERSION}-{target}.sha256");
            let checksum_asset = release
                .assets
                .iter()
                .find(|asset| asset.name == checksum_name)
                .ok_or_else(|| format!("Missing release checksum: {checksum_name}"))?;
            let checksum = zed::http_client::HttpRequest::builder()
                .method(zed::http_client::HttpMethod::Get)
                .url(&checksum_asset.download_url)
                .redirect_policy(zed::http_client::RedirectPolicy::FollowAll)
                .build()?
                .fetch()?;
            let checksum =
                String::from_utf8(checksum.body).map_err(|_| "Invalid release checksum")?;
            let expected = checksum
                .split_whitespace()
                .next()
                .ok_or("Empty release checksum")?;
            let temporary = format!("{directory}.download");
            if Path::new(&temporary).exists() {
                fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
            }
            zed::download_file(
                &asset.download_url,
                &temporary,
                zed::DownloadedFileType::GzipTar,
            )?;
            let contents =
                fs::read(format!("{temporary}/sendline")).map_err(|error| error.to_string())?;
            if format!("{:x}", Sha256::digest(contents)) != expected {
                let _ = fs::remove_dir_all(&temporary);
                return Err("Sendline download failed its SHA-256 integrity check.".into());
            }
            zed::make_file_executable(&format!("{temporary}/sendline"))?;
            fs::rename(&temporary, &directory).map_err(|error| error.to_string())?;
        }
        Ok(binary)
    }
}

impl zed::Extension for SendlineExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.binary(id, worktree)?,
            args: vec!["lsp".into(), "--extension-version".into(), VERSION.into()],
            env: vec![],
        })
    }
}

zed::register_extension!(SendlineExtension);
