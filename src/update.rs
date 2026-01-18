//! Runtime-only public GitHub HTTPS access. Tests use release fixtures, never the network.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const REPOSITORY: &str = "https://github.com/siarheisinelnikau/second-screen";
const API: &str = "https://api.github.com/repos/siarheisinelnikau/second-screen/releases/latest";
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct Available {
    pub version: semver::Version,
    pub url: String,
    pub size: u64,
    pub digest: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

fn agent(seconds: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(seconds)))
        .build()
        .into()
}
pub fn select_release(json: &[u8], installed: &str) -> Result<Option<Available>, String> {
    let release: Release =
        serde_json::from_slice(json).map_err(|e| format!("Invalid release response: {e}"))?;
    let version = semver::Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|e| e.to_string())?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Ok(None);
    }
    if version <= semver::Version::parse(installed).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Automatic installation currently supports Windows x64 only".into());
    }
    let expected_name = format!("SecondScreen-v{version}.exe");
    let expected_url = format!(
        "{REPOSITORY}/releases/download/{}/{expected_name}",
        release.tag_name
    );
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == expected_name)
        .ok_or("Release has no compatible portable executable")?;
    if asset.browser_download_url != expected_url {
        return Err("Release asset URL is outside the canonical release".into());
    }
    if asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err("Release executable has an invalid size".into());
    }
    let digest = asset
        .digest
        .and_then(|d| d.strip_prefix("sha256:").map(str::to_owned))
        .ok_or("Release has no SHA-256 digest; automatic install is unavailable")?;
    if !valid_digest(&digest) {
        return Err("Invalid release SHA-256 digest".into());
    }
    Ok(Some(Available {
        version,
        url: asset.browser_download_url,
        size: asset.size,
        digest: digest.to_ascii_lowercase(),
    }))
}
pub fn check() -> Result<Option<Available>, String> {
    let mut response = agent(15)
        .get(API)
        .header("User-Agent", "SecondScreen/0.1.0")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| {
            format!("Update check unavailable (repository/release may not be published): {e}")
        })?;
    let mut data = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 1024 * 1024 {
        return Err("Release response exceeds 1 MiB".into());
    }
    select_release(&data, crate::VERSION)
}
fn valid_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn verify(path: &Path, digest: &str) -> Result<(), String> {
    if !valid_digest(digest) {
        return Err("Invalid SHA-256 digest".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size < 64 || size > MAX_DOWNLOAD {
        return Err("Invalid executable size".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    if format!("{:x}", hasher.finalize()) != digest.to_ascii_lowercase() {
        return Err("SHA-256 mismatch; installed application was not changed".into());
    }
    Ok(())
}
fn verify_pe(path: &Path) -> Result<(), String> {
    use std::io::{Seek, SeekFrom};
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0; 64];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..2] != b"MZ" {
        return Err("Downloaded asset is not a Windows executable".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
    if offset > MAX_DOWNLOAD - 6 {
        return Err("Invalid PE header offset".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0; 6];
    file.read_exact(&mut pe).map_err(|e| e.to_string())?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664 {
        return Err("Release executable is not Windows x64".into());
    }
    Ok(())
}

/// Downloads into the executable directory. No installation or helper is started here.
pub fn download(
    update: &Available,
    executable: &Path,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf, String> {
    let folder = executable
        .parent()
        .ok_or("Cannot find executable directory")?;
    let staged = folder.join(format!(
        ".second-screen-update-{}.exe",
        crate::pairing::secret()
    ));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|e| {
                format!("Cannot write next to the portable EXE. Move it to a writable folder: {e}")
            })?;
        let mut response = agent(180)
            .get(&update.url)
            .header("User-Agent", "SecondScreen/0.1.0")
            .call()
            .map_err(|e| e.to_string())?;
        let reader = response.body_mut().as_reader();
        let mut total = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > update.size || total > MAX_DOWNLOAD {
                return Err("Download exceeds the release size".into());
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            progress(total, update.size);
        }
        if total != update.size {
            return Err("Incomplete download; the installed application was not changed".into());
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        verify(&staged, &update.digest)?;
        verify_pe(&staged)?;
        Ok(staged.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(windows)]
pub fn launch_helper(staged: &Path, digest: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let folder = executable.parent().ok_or("Missing executable directory")?;
    let helper = folder.join(format!(
        ".second-screen-helper-{}.exe",
        crate::pairing::secret()
    ));
    fs::copy(&executable, &helper).map_err(|e| e.to_string())?;
    let result = std::process::Command::new(&helper)
        .arg("--apply-update")
        .arg(std::process::id().to_string())
        .arg(&executable)
        .arg(staged)
        .arg(digest)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("Cannot start update helper: {e}"));
    if result.is_err() {
        let _ = fs::remove_file(helper);
    }
    result.map(|_| ())
}
#[cfg(not(windows))]
pub fn launch_helper(_: &Path, _: &str) -> Result<(), String> {
    Err("Updater requires Windows".into())
}

/// Runs only when explicitly invoked by the application after Download and install.
#[cfg(windows)]
pub fn apply_helper(args: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            QueryFullProcessImageNameW, WaitForSingleObject,
        },
    };
    if args.len() != 4 {
        return Err("Invalid updater arguments".into());
    }
    let pid: u32 = args[0].parse().map_err(|_| "Invalid parent process")?;
    let target = fs::canonicalize(&args[1]).map_err(|e| e.to_string())?;
    let staged = fs::canonicalize(&args[2]).map_err(|e| e.to_string())?;
    let helper = fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if staged.parent() != target.parent()
        || helper.parent() != target.parent()
        || !staged
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(".second-screen-update-")
        || staged == target
        || target == helper
    {
        return Err("Update paths must be distinct and in the portable application folder".into());
    }
    verify(&staged, &args[3])?;
    verify_pe(&staged)?;
    // SAFETY: process handle is checked and closed; image buffer length is explicit.
    unsafe {
        let process = OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if process.is_null() {
            return Err("Parent process is unavailable; update was not installed".into());
        }
        let mut name = vec![0u16; 32768];
        let mut len = name.len() as u32;
        let queried = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len);
        let process_path = if queried != 0 {
            fs::canonicalize(String::from_utf16_lossy(&name[..len as usize])).ok()
        } else {
            None
        };
        if process_path.as_ref() != Some(&target) {
            CloseHandle(process);
            return Err("Parent process does not match the installed executable".into());
        }
        let waited = WaitForSingleObject(process, 30000);
        CloseHandle(process);
        if waited != WAIT_OBJECT_0 {
            return Err(
                "Application did not exit within 30 seconds; update was not installed".into(),
            );
        }
    }
    verify(&staged, &args[3])?;
    let backup = target.with_extension(format!("previous-{}.exe", crate::pairing::secret()));
    fs::rename(&target, &backup)
        .map_err(|e| format!("Cannot back up installed application: {e}"))?;
    if let Err(e) = fs::rename(&staged, &target) {
        let _ = fs::rename(&backup, &target);
        return Err(format!("Cannot install update; attempted rollback: {e}"));
    }
    if let Err(e) = std::process::Command::new(&target)
        .creation_flags(0x08000000)
        .spawn()
    {
        let _ = fs::rename(&target, &staged);
        let _ = fs::rename(&backup, &target);
        return Err(format!("Restart failed; attempted rollback: {e}"));
    }
    // Leave a backup for recovery. The helper cannot delete its own running file.
    Ok(())
}
#[cfg(not(windows))]
pub fn apply_helper(_: &[String]) -> Result<(), String> {
    Err("Updater requires Windows".into())
}

pub fn record_helper_error(error: &str) {
    // Helper failures stay visible after the parent exits, without a hidden console.
    if let Some(parent) = crate::settings::path().parent() {
        let _ = fs::create_dir_all(parent);
        let _ = fs::write(parent.join("update-error.txt"), error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(version: &str, digest: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"assets":[{"name":format!("SecondScreen-v{version}.exe"),"browser_download_url":format!("{REPOSITORY}/releases/download/v{version}/SecondScreen-v{version}.exe"),"size":1024,"digest":digest}]})).unwrap()
    }
    #[test]
    fn numeric_versions_and_canonical_asset_selection() {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert!(
            select_release(&fixture("0.1.0", &digest), "0.1.0")
                .unwrap()
                .is_none()
        );
        assert!(
            select_release(&fixture("0.2.0-beta.1", &digest), "0.1.0")
                .unwrap()
                .is_none()
        );
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            assert_eq!(
                select_release(&fixture("0.10.0", &digest), "0.9.0")
                    .unwrap()
                    .unwrap()
                    .version
                    .to_string(),
                "0.10.0"
            );
            assert!(select_release(&fixture("0.10.0", "sha256:bad"), "0.9.0").is_err());
            let modified = String::from_utf8(fixture("0.10.0", &digest))
                .unwrap()
                .replace("https://github.com/", "https://example.com/");
            assert!(select_release(modified.as_bytes(), "0.9.0").is_err());
        }
    }
    #[test]
    fn corrupt_download_is_rejected_without_touching_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.exe");
        fs::write(&path, [42u8; 128]).unwrap();
        assert!(verify(&path, &"0".repeat(64)).is_err());
        let digest = format!("{:x}", Sha256::digest([42u8; 128]));
        verify(&path, &digest).unwrap();
        assert!(verify_pe(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), vec![42; 128]);
    }
}
