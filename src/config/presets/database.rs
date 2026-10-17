//! config presets database module.
//!
//! Contains config presets database logic used by Berth command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &[
    "mongodb",
    "postgres",
    "pg",
    "pgsql",
    "mysql",
    "mariadb",
    "sqlserver",
    "mssql",
];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "mongodb" => Some(mongodb()),
        "postgres" | "pg" | "pgsql" => Some(postgres()),
        "mysql" => Some(mysql()),
        "mariadb" => Some(mariadb()),
        "sqlserver" | "mssql" => Some(sqlserver()),
        _ => None,
    }
}

fn mongodb() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Database, Driver::Mongodb, "mongo:8");
    defaults.name = Some("mongodb");
    defaults.database = Some("laravel");
    defaults.username = Some("laravel");
    defaults.password = Some("laravel");
    defaults
}

fn postgres() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Database,
        Driver::Postgres,
        "timescale/timescaledb-ha:pg18",
    );
    defaults.name = Some("db");
    defaults.database = Some("laravel");
    defaults.username = Some("laravel");
    defaults.password = Some("laravel");
    defaults
}

fn mysql() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Database, Driver::Mysql, "mysql:8.1");
    defaults.name = Some("db");
    defaults.database = Some("laravel");
    defaults.username = Some("laravel");
    defaults.password = Some("laravel");
    defaults
}

fn mariadb() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Database, Driver::Mysql, "mariadb:11");
    defaults.name = Some("db");
    defaults.database = Some("laravel");
    defaults.username = Some("laravel");
    defaults.password = Some("laravel");
    defaults
}

fn sqlserver() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Database,
        Driver::Sqlserver,
        "mcr.microsoft.com/mssql/server:2022-latest",
    );
    defaults.name = Some("db");
    defaults.database = Some("laravel");
    defaults.username = Some("sa");
    defaults.password = Some("BerthSqlServerPassw0rd!");
    defaults
}
