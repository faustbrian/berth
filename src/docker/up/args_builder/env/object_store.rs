//! docker up args builder env object store module.
//!
//! Contains docker up args builder env object store logic used by Berth command workflows.

use crate::config::{Driver, ServiceConfig};

/// Appends append to the caller-provided command or collection.
pub(super) fn append(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Minio => append_minio(args, service),
        Driver::Garage => append_minio(args, service),
        Driver::Localstack => append_localstack(args, service),
        Driver::Rustfs => append_rustfs(args, service),
        _ => {}
    }
}

/// Appends minio to the caller-provided command or collection.
fn append_minio(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(access_key) = &service.access_key {
        args.push("-e".to_owned());
        args.push(format!("MINIO_ROOT_USER={access_key}"));
    }
    if let Some(secret_key) = &service.secret_key {
        args.push("-e".to_owned());
        args.push(format!("MINIO_ROOT_PASSWORD={secret_key}"));
    }
}

/// Appends rustfs to the caller-provided command or collection.
fn append_rustfs(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(access_key) = &service.access_key {
        args.push("-e".to_owned());
        args.push(format!("RUSTFS_ACCESS_KEY={access_key}"));
    }
    if let Some(secret_key) = &service.secret_key {
        args.push("-e".to_owned());
        args.push(format!("RUSTFS_SECRET_KEY={secret_key}"));
    }
    if let Some(region) = &service.region {
        args.push("-e".to_owned());
        args.push(format!("RUSTFS_REGION={region}"));
    }
    args.push("-e".to_owned());
    args.push("RUSTFS_VOLUMES=/data".to_owned());
}

fn append_localstack(args: &mut Vec<String>, service: &ServiceConfig) {
    args.push("-e".to_owned());
    args.push("SERVICES=s3".to_owned());
    args.push("-e".to_owned());
    args.push(format!(
        "AWS_DEFAULT_REGION={}",
        service.region.as_deref().unwrap_or("us-east-1")
    ));
}
