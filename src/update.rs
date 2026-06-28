//! SecondScreen release contract; reusable update logic lives in rust-auto-updator.
pub use rust_auto_updator::{Prepared, Release as Available};

#[cfg(test)]
const REPOSITORY: &str = "https://github.com/siarheisinelnikau/second-screen";

fn config_for(version: &str) -> Result<rust_auto_updator::Config, String> {
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "second-screen",
        version,
        "SecondScreen",
    )?;
    config.expected_subsystem = Some(2);
    config.install.startup_health_timeout = Some(std::time::Duration::from_secs(30));
    config.max_executable_size = 200 * 1024 * 1024;
    // Keep the package checksum filename and earlier digest-only releases compatible.
    config.checksum_name = "SHA256SUMS".into();
    config.allow_asset_digest = true;
    config.install.stage_prefix = ".second-screen-update-".into();
    config.install.helper_name = "SecondScreen.exe".into();
    Ok(config)
}

pub fn check() -> Result<Option<Available>, String> {
    rust_auto_updator::check_latest(&config_for(crate::VERSION)?)
}

pub fn download(update: &Available, progress: impl FnMut(u64, u64)) -> Result<Prepared, String> {
    rust_auto_updator::prepare_download(update, &config_for(crate::VERSION)?, progress)
}

pub fn launch_helper(prepared: &mut Prepared) -> Result<(), String> {
    rust_auto_updator::handoff(prepared)
}

pub fn apply_helper() -> Result<(), String> {
    rust_auto_updator::apply_update(&config_for(crate::VERSION)?)
}

pub fn cleanup_after_update(args: &[String]) {
    if args.first().is_some_and(|arg| arg == "--cleanup-update")
        && let Some(directory) = args.get(1)
        && let Ok(config) = config_for(crate::VERSION)
    {
        rust_auto_updator::cleanup_after_update(directory.into(), config.install);
    }
}

pub fn record_helper_error(error: &str) {
    if let Some(parent) = crate::settings::path().parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(parent.join("update-error.txt"), error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_only_legacy_releases_and_repository_binding() {
        let config = config_for("0.2.0").unwrap();
        let mut fixture = serde_json::json!({
            "tag_name":"v0.3.0", "draft":false, "prerelease":false,
            "html_url":format!("{REPOSITORY}/releases/tag/v0.3.0"),
            "assets":[{"name":"SecondScreen-v0.3.0.exe", "size":4096,
                "browser_download_url":format!("{REPOSITORY}/releases/download/v0.3.0/SecondScreen-v0.3.0.exe"),
                "digest":format!("sha256:{}", "ab".repeat(32))}]
        });
        assert!(
            rust_auto_updator::parse_release(&fixture.to_string(), &config)
                .unwrap()
                .is_some()
        );
        fixture["assets"][0]["browser_download_url"] = "https://example.com/app.exe".into();
        assert!(rust_auto_updator::parse_release(&fixture.to_string(), &config).is_err());
    }
}
