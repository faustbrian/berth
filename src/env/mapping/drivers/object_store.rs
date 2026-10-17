//! env mapping drivers object store module.
//!
//! Contains env mapping drivers object store logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config::ServiceConfig;

pub(super) fn apply_object_store_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("FILESYSTEM_DISK".to_owned(), "s3".to_owned());
    map.insert(
        "AWS_ACCESS_KEY_ID".to_owned(),
        service
            .access_key
            .clone()
            .unwrap_or_else(|| "minio".to_owned()),
    );
    map.insert(
        "AWS_SECRET_ACCESS_KEY".to_owned(),
        service
            .secret_key
            .clone()
            .unwrap_or_else(|| "miniosecret".to_owned()),
    );
    map.insert(
        "AWS_DEFAULT_REGION".to_owned(),
        service
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_owned()),
    );
    map.insert(
        "AWS_BUCKET".to_owned(),
        service.bucket.clone().unwrap_or_else(|| "app".to_owned()),
    );
    map.insert(
        "AWS_ENDPOINT".to_owned(),
        format!("{}://{}:{}", service.scheme(), service.host, service.port),
    );
    map.insert("AWS_USE_PATH_STYLE_ENDPOINT".to_owned(), "true".to_owned());
}
