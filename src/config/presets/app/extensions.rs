//! config presets app extensions module.
//!
//! Contains config presets app extensions logic used by Berth command workflows.

pub(super) fn laravel_minimal_extensions() -> Vec<String> {
    vec![
        "ctype".to_owned(),
        "dom".to_owned(),
        "curl".to_owned(),
        "libxml".to_owned(),
        "mbstring".to_owned(),
        "sockets".to_owned(),
        "tokenizer".to_owned(),
        "zip".to_owned(),
        "pdo".to_owned(),
        "pdo_mysql".to_owned(),
    ]
}

pub(super) fn laravel_extensions() -> Vec<String> {
    vec![
        "ctype".to_owned(),
        "dom".to_owned(),
        "curl".to_owned(),
        "libxml".to_owned(),
        "mbstring".to_owned(),
        "tokenizer".to_owned(),
        "zip".to_owned(),
        "pcntl".to_owned(),
        "sockets".to_owned(),
        "calendar".to_owned(),
        "pdo".to_owned(),
        "pdo_pgsql".to_owned(),
        "sqlite".to_owned(),
        "pdo_sqlite".to_owned(),
        "redis".to_owned(),
        "bcmath".to_owned(),
        "soap".to_owned(),
        "intl".to_owned(),
        "gd".to_owned(),
        "exif".to_owned(),
        "iconv".to_owned(),
        "imagick".to_owned(),
        "fileinfo".to_owned(),
        "xsl".to_owned(),
        "pdo_mysql".to_owned(),
    ]
}
