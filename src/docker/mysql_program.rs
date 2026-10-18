//! The MySQL-family client programs an image ships.

/// A MySQL-family client program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MysqlProgram {
    /// The SQL shell (`mysql`, `mariadb`).
    Client,
    /// The admin tool (`mysqladmin`, `mariadb-admin`).
    Admin,
    /// The dump tool (`mysqldump`, `mariadb-dump`).
    Dump,
}

/// The program's name in `image`. MariaDB 11 images ship only the
/// `mariadb*` names (the `mysql*` aliases were removed), while MariaDB
/// before 10.4.6 ships only `mysql*`; 10.x images use the `mysql*` names,
/// which every 10.x release ships, and 11 and later (or an untagged image)
/// the `mariadb*` names.
#[must_use]
pub(crate) fn mysql_program(image: &str, program: MysqlProgram) -> &'static str {
    let mariadb = is_mariadb_image(image) && !is_mariadb_10_or_older(image);
    match (program, mariadb) {
        (MysqlProgram::Client, false) => "mysql",
        (MysqlProgram::Client, true) => "mariadb",
        (MysqlProgram::Admin, false) => "mysqladmin",
        (MysqlProgram::Admin, true) => "mariadb-admin",
        (MysqlProgram::Dump, false) => "mysqldump",
        (MysqlProgram::Dump, true) => "mariadb-dump",
    }
}

/// Whether the image's tag names MariaDB 10 or older (`10.11`, `10-jammy`).
fn is_mariadb_10_or_older(image: &str) -> bool {
    let without_digest = image.split('@').next().unwrap_or(image);
    let Some((_, tag)) = without_digest
        .rsplit_once(':')
        .filter(|(_, tag)| !tag.contains('/'))
    else {
        return false;
    };
    tag.split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major <= 10)
}

/// Whether the image's repository (without registry, tag or digest) names
/// MariaDB.
#[must_use]
pub(crate) fn is_mariadb_image(image: &str) -> bool {
    let without_digest = image.split('@').next().unwrap_or(image);
    let repository = match without_digest.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => repository,
        _ => without_digest,
    };
    repository
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains("mariadb"))
}

/// How a MySQL-family client inside a server's own container reaches it:
/// over TCP to the loopback. The socket path is not fixed (rootless Docker
/// and Podman in CI ran mysqld without the image's `/etc/my.cnf`, on
/// `/var/lib/mysql/mysql.sock` instead of the clients' default), and the
/// image's temporary init server listens on the socket only.
pub(crate) const MYSQL_LOCAL_TCP: [&str; 3] = ["--protocol=tcp", "-h", "127.0.0.1"];

#[cfg(test)]
mod tests {
    use super::{MysqlProgram, is_mariadb_image, mysql_program};

    #[test]
    fn mariadb_images_get_mariadb_programs() {
        assert!(is_mariadb_image("mariadb:11"));
        assert!(is_mariadb_image("docker.io/library/mariadb@sha256:abc"));
        assert!(!is_mariadb_image("mysql:8.4"));
        assert!(!is_mariadb_image("registry:5000/mysql:mariadb-compat"));
        assert_eq!(
            mysql_program("mariadb:11", MysqlProgram::Admin),
            "mariadb-admin"
        );
        assert_eq!(mysql_program("mysql:8.4", MysqlProgram::Dump), "mysqldump");
        assert_eq!(
            mysql_program("mariadb:10.3", MysqlProgram::Admin),
            "mysqladmin"
        );
        assert_eq!(
            mysql_program("mariadb:10.11-jammy", MysqlProgram::Client),
            "mysql"
        );
        assert_eq!(
            mysql_program("mariadb:lts", MysqlProgram::Client),
            "mariadb"
        );
    }
}
