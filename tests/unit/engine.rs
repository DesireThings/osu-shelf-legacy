use super::{NOT_A_COLLECTION_LINK, Settings, default_download_dir_in, preview_collection};
use crate::config::Config;
use crate::osu_db::OsuClient;
use std::path::PathBuf;

#[test]
fn settings_round_trip_through_the_shared_config() {
    let settings = Settings {
        client: OsuClient::Stable,
        osu_path: Some(PathBuf::from("/games/osu!")),
        download_dir: PathBuf::from("/maps"),
    };
    let mut config = Config::default();

    settings.apply_to(&mut config);

    assert_eq!(config.recent.osu_client, Some(OsuClient::Stable));
    assert_eq!(config.recent.osu_path.as_deref(), Some("/games/osu!"));
    assert_eq!(config.recent.directory.as_deref(), Some("/maps"));
    assert_eq!(Settings::from_config(&config), settings);
}

#[test]
fn empty_config_defaults_to_lazer_autodetect_and_a_download_folder() {
    let settings = Settings::from_config(&Config::default());

    assert_eq!(settings.client, OsuClient::Lazer);
    assert_eq!(settings.osu_path, None);
    let name = settings
        .download_dir
        .file_name()
        .and_then(|name| name.to_str());
    assert!(matches!(name, Some("osu-shelf" | "osu-collect")));
}

#[test]
fn download_dir_uses_the_new_folder_on_a_fresh_install() {
    let base = tempfile::tempdir().unwrap();

    assert_eq!(
        default_download_dir_in(base.path().to_path_buf()),
        base.path().join("osu-shelf")
    );
}

#[test]
fn download_dir_falls_back_to_an_existing_legacy_folder() {
    let base = tempfile::tempdir().unwrap();
    std::fs::create_dir(base.path().join("osu-collect")).unwrap();

    assert_eq!(
        default_download_dir_in(base.path().to_path_buf()),
        base.path().join("osu-collect")
    );
}

#[test]
fn download_dir_prefers_the_new_folder_when_both_exist() {
    let base = tempfile::tempdir().unwrap();
    std::fs::create_dir(base.path().join("osu-collect")).unwrap();
    std::fs::create_dir(base.path().join("osu-shelf")).unwrap();

    assert_eq!(
        default_download_dir_in(base.path().to_path_buf()),
        base.path().join("osu-shelf")
    );
}

#[test]
fn tilde_in_the_stored_directory_is_expanded() {
    let mut config = Config::default();
    config.recent.directory = Some("~/maps".to_string());

    let settings = Settings::from_config(&config);

    assert!(!settings.download_dir.starts_with("~"));
    assert!(settings.download_dir.ends_with("maps"));
}

#[test]
fn fetch_failures_get_a_message_that_says_what_to_do() {
    use osu_downloader::Error;
    let not_found = super::friendly_fetch_error(&Error::NotFound);
    assert!(not_found.contains("doesn't exist"));
    let limited = super::friendly_fetch_error(&Error::RateLimited { retry_after: None });
    assert!(limited.contains("try again"));
    let offline = super::friendly_fetch_error(&Error::Network("dns error".into()));
    assert!(!offline.contains("dns"));
    assert_eq!(offline, super::friendly_fetch_error(&Error::Timeout));
    assert!(!super::friendly_fetch_error(&Error::HttpStatus(502)).contains("502"));
}

#[tokio::test]
async fn preview_rejects_input_that_is_not_a_collection_link() {
    let err = preview_collection("definitely not a link")
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), NOT_A_COLLECTION_LINK);
}
