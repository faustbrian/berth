//! docker up args builder env sql module.
//!
//! Contains docker up args builder env sql logic used by Berth command workflows.

use crate::config::{Driver, ServiceConfig};

/// Appends append to the caller-provided command or collection.
pub(super) fn append(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Mongodb => append_mongodb(args, service),
        Driver::Postgres => append_postgres(args, service),
        Driver::Mysql => append_mysql(args, service),
        Driver::Sqlserver => append_sqlserver(args, service),
        _ => {}
    }
}

fn append_mongodb(args: &mut Vec<String>, service: &ServiceConfig) {
    let username = service.username.as_deref().unwrap_or("root");
    let password = service.password.as_deref().unwrap_or("secret");
    args.push("-e".to_owned());
    args.push(format!("MONGO_INITDB_ROOT_USERNAME={username}"));
    args.push("-e".to_owned());
    args.push(format!("MONGO_INITDB_ROOT_PASSWORD={password}"));
    args.push("-e".to_owned());
    args.push(format!(
        "MONGO_INITDB_DATABASE={}",
        service.database.as_deref().unwrap_or("app")
    ));
}

/// Appends postgres to the caller-provided command or collection.
fn append_postgres(args: &mut Vec<String>, service: &ServiceConfig) {
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_USER={}",
        service.username.as_deref().unwrap_or("postgres")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_PASSWORD={}",
        service.password.as_deref().unwrap_or("secret")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_DB={}",
        service.database.as_deref().unwrap_or("app")
    ));
}

/// Appends mysql to the caller-provided command or collection.
fn append_mysql(args: &mut Vec<String>, service: &ServiceConfig) {
    let password = service.password.as_deref().unwrap_or("secret");
    args.push("-e".to_owned());
    args.push(format!("MYSQL_ROOT_PASSWORD={password}"));
    args.push("-e".to_owned());
    args.push(format!(
        "MYSQL_DATABASE={}",
        service.database.as_deref().unwrap_or("app")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "MYSQL_USER={}",
        service.username.as_deref().unwrap_or("root")
    ));
    args.push("-e".to_owned());
    args.push(format!("MYSQL_PASSWORD={password}"));
}

fn append_sqlserver(args: &mut Vec<String>, service: &ServiceConfig) {
    args.push("-e".to_owned());
    args.push("ACCEPT_EULA=Y".to_owned());
    args.push("-e".to_owned());
    args.push(format!(
        "MSSQL_SA_PASSWORD={}",
        service
            .password
            .as_deref()
            .unwrap_or("BerthSqlServerPassw0rd!")
    ));
    args.push("-e".to_owned());
    args.push("MSSQL_PID=Developer".to_owned());
}
