//! Hides secrets in docker command lines that berth prints or embeds in
//! errors (dry runs, "runtime command failed: …"). The real arguments are
//! never changed.

/// Environment keys whose values are treated as secrets.
const SECRET_MARKERS: [&str; 8] = [
    "AUTH",
    "PASSWORD",
    "SECRET",
    "TOKEN",
    "KEY",
    "PASS",
    "PWD",
    "CREDENTIAL",
];

const REDACTED: &str = "<redacted>";

/// Secret values berth knows (configured credentials, the managed
/// `APP_KEY`). They are hidden only where a value stands on its own (the
/// whole value of a `NAME=value` argument), never inside free text: preset
/// defaults are ordinary words (`laravel`) that also name paths and images.
static SECRETS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Registers a secret value. Values shorter than 8 characters are ignored;
/// so are values equal to a name that is not secret (see the callers).
pub(crate) fn register_secret(value: &str) {
    if value.len() < 8 {
        return;
    }
    let mut secrets = SECRETS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !secrets.iter().any(|known| known == value) {
        secrets.push(value.to_owned());
    }
}

/// Hides URL passwords (`scheme://user:password@host`) anywhere in `text`.
/// Used for free text: errors, log events and the failure document.
#[must_use]
pub(crate) fn redact_known_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(scheme_end) = rest.find("://") {
        let authority_start = scheme_end + 3;
        out.push_str(&rest[..authority_start]);
        rest = &rest[authority_start..];
        let authority_end = rest
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        match authority
            .rfind('@')
            .and_then(|at| authority[..at].find(':').map(|colon| (colon, at)))
        {
            Some((colon, at)) => {
                out.push_str(&authority[..=colon]);
                out.push_str(REDACTED);
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &rest[authority_end..];
    }
    out.push_str(rest);
    out
}

/// Whether `value` is a registered secret.
fn is_known_secret(value: &str) -> bool {
    SECRETS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|secret| secret == value)
}

/// Returns `args` with secret values replaced: `-e NAME=value` for secret
/// names, `--env NAME=value`, and MySQL's attached `-p<password>`.
#[must_use]
pub(crate) fn redact_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut previous = "";
    for arg in args {
        let value = if previous == "-e" || previous == "--env" {
            redact_assignment(arg)
        } else if previous == "-P" || previous == "--password" {
            // `sqlcmd -P <password>`, `--password <password>`.
            REDACTED.to_owned()
        } else if let Some(assignment) = arg.strip_prefix("--env=") {
            format!("--env={}", redact_assignment(assignment))
        } else if arg.starts_with("--password=") {
            format!("--password={REDACTED}")
        } else if arg.len() > 2 && arg.starts_with("-p") && !arg.starts_with("-p=") {
            // `-p` followed by a separate value is a port mapping; an
            // attached value is a MySQL client password.
            format!("-p{REDACTED}")
        } else {
            arg.clone()
        };
        previous = arg;
        let value = if is_known_secret(&value) {
            REDACTED.to_owned()
        } else {
            value
        };
        redacted.push(redact_known_secrets(&value));
    }
    redacted
}

fn redact_assignment(assignment: &str) -> String {
    match assignment.split_once('=') {
        Some((name, value))
            if !value.is_empty() && (is_secret_name(name) || is_known_secret(value)) =>
        {
            format!("{name}={REDACTED}")
        }
        _ => assignment.to_owned(),
    }
}

/// Whether `NAME=value` carries a secret: a secret name or a registered
/// secret value.
pub(crate) fn is_secret_assignment(name: &str, value: &str) -> bool {
    !value.is_empty() && (is_secret_name(name) || is_known_secret(value))
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_MARKERS.iter().any(|marker| upper.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::redact_args;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn known_secrets_are_hidden_inside_any_value_and_password_flags_too() {
        super::register_secret("canary-in-url");
        let redacted = redact_args(&args(&[
            "-e",
            "DATABASE_URL=postgres://u:canary-in-url@db/x",
            "sqlcmd",
            "-P",
            "pw0rd",
            "--password=topsecret",
        ]));
        assert_eq!(
            redacted,
            args(&[
                "-e",
                "DATABASE_URL=postgres://u:<redacted>@db/x",
                "sqlcmd",
                "-P",
                "<redacted>",
                "--password=<redacted>",
            ])
        );
    }

    #[test]
    fn secret_env_values_and_mysql_passwords_are_hidden_and_the_rest_kept() {
        let redacted = redact_args(&args(&[
            "run",
            "-e",
            "POSTGRES_PASSWORD=hunter2",
            "-e",
            "DB_HOST=db",
            "-e",
            "APP_KEY=base64:x",
            "--env=AWS_SECRET_ACCESS_KEY=s3",
            "-p",
            "127.0.0.1:5432:5432",
            "mysql",
            "-uroot",
            "-psecret",
        ]));
        assert_eq!(
            redacted,
            args(&[
                "run",
                "-e",
                "POSTGRES_PASSWORD=<redacted>",
                "-e",
                "DB_HOST=db",
                "-e",
                "APP_KEY=<redacted>",
                "--env=AWS_SECRET_ACCESS_KEY=<redacted>",
                "-p",
                "127.0.0.1:5432:5432",
                "mysql",
                "-uroot",
                "-p<redacted>",
            ])
        );
    }
}
