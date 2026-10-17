use super::super::merge_with_protected_https_app_urls;
use std::collections::HashMap;

#[test]
fn merge_blocks_http_downgrade_for_app_url() {
    let mut base = HashMap::from([("APP_URL".to_owned(), "https://app.berth".to_owned())]);
    let incoming = HashMap::from([("APP_URL".to_owned(), "http://app.berth".to_owned())]);

    merge_with_protected_https_app_urls(&mut base, &incoming);

    assert_eq!(base.get("APP_URL"), Some(&"https://app.berth".to_owned()));
}

#[test]
fn merge_blocks_http_downgrade_for_asset_url() {
    let mut base = HashMap::from([("ASSET_URL".to_owned(), "https://app.berth".to_owned())]);
    let incoming = HashMap::from([("ASSET_URL".to_owned(), "http://app.berth".to_owned())]);

    merge_with_protected_https_app_urls(&mut base, &incoming);

    assert_eq!(base.get("ASSET_URL"), Some(&"https://app.berth".to_owned()));
}

#[test]
fn merge_allows_https_override() {
    let mut base = HashMap::from([("APP_URL".to_owned(), "https://old.berth".to_owned())]);
    let incoming = HashMap::from([("APP_URL".to_owned(), "https://new.berth".to_owned())]);

    merge_with_protected_https_app_urls(&mut base, &incoming);

    assert_eq!(base.get("APP_URL"), Some(&"https://new.berth".to_owned()));
}

#[test]
fn merge_allows_non_url_overrides() {
    let mut base = HashMap::from([("FOO".to_owned(), "one".to_owned())]);
    let incoming = HashMap::from([("FOO".to_owned(), "two".to_owned())]);

    merge_with_protected_https_app_urls(&mut base, &incoming);

    assert_eq!(base.get("FOO"), Some(&"two".to_owned()));
}
