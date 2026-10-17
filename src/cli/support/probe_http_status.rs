//! cli support probe http status module.
//!
//! Contains cli support probe http status logic used by Berth command workflows.

use std::process::Command;

#[cfg(test)]
thread_local! {
    static FAKE_CURL_COMMAND: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
}

fn curl_command() -> String {
    #[cfg(test)]
    {
        FAKE_CURL_COMMAND.with(|command| {
            command
                .borrow()
                .clone()
                .unwrap_or_else(|| String::from("curl"))
        })
    }
    #[cfg(not(test))]
    {
        String::from("curl")
    }
}

pub(crate) fn run_curl_command(url: &str) -> Option<std::process::Output> {
    Command::new(curl_command())
        .args(["-k", "-sS", "-w", "\n%{http_code}", "--max-time", "5", url])
        .output()
        .ok()
}

pub(crate) fn run_curl_command_with_body(url: &str) -> Option<std::process::Output> {
    Command::new(curl_command())
        .args(["-k", "-sS", "-w", "\n%{http_code}", "--max-time", "5", url])
        .output()
        .ok()
}

#[cfg(test)]
pub(crate) fn with_curl_command<F, T>(command: &str, test: F) -> T
where
    F: FnOnce() -> T,
{
    let previous = FAKE_CURL_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        let previous = current.clone();
        *current = Some(command.to_owned());
        previous
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    FAKE_CURL_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        *current = previous;
    });

    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

pub(crate) fn probe_http_status(url: &str) -> Option<u16> {
    let output = run_curl_command(url)?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    raw.lines().next_back()?.trim().parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::with_curl_command;
    use super::{probe_http_status, run_curl_command_with_body};
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_fake_curl(script_body: &str, test_url: &str) -> Option<u16> {
        let tmp = env::temp_dir().join(format!(
            "berth-curl-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).expect("temp dir");
        let curl = tmp.join("curl");
        let mut file = fs::File::create(&curl).expect("curl script");
        writeln!(file, "#!/bin/sh\n{}", script_body).expect("write script");
        let mut perms = fs::metadata(&curl).expect("metadata").permissions();
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&curl, perms).expect("executable script");
        }

        let command = curl.to_string_lossy().to_string();
        let status = with_curl_command(&command, || probe_http_status(test_url));
        fs::remove_dir_all(&tmp).ok();
        status
    }

    fn with_fake_curl_output(script_body: &str, test_url: &str) -> Option<std::process::Output> {
        let tmp = env::temp_dir().join(format!(
            "berth-curl-output-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).expect("temp dir");
        let curl = tmp.join("curl");
        let mut file = fs::File::create(&curl).expect("curl script");
        writeln!(file, "#!/bin/sh\n{}", script_body).expect("write script");
        let mut perms = fs::metadata(&curl).expect("metadata").permissions();
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&curl, perms).expect("executable script");
        }

        let command = curl.to_string_lossy().to_string();
        let output = with_curl_command(&command, || run_curl_command_with_body(test_url));
        fs::remove_dir_all(&tmp).ok();
        output
    }

    #[test]
    fn probe_http_status_returns_status_code_on_success() {
        assert_eq!(
            with_fake_curl("printf '%s' '200'", "https://app.berth"),
            Some(200)
        );
    }

    #[test]
    fn probe_http_status_returns_none_when_command_fails() {
        assert_eq!(with_fake_curl("exit 1", "https://app.berth"), None);
    }

    #[test]
    fn probe_http_status_returns_none_for_invalid_body() {
        assert_eq!(
            with_fake_curl("printf '%s' 'not-a-code'", "https://app.berth"),
            None
        );
    }

    #[test]
    fn probe_http_status_reads_status_from_last_output_line() {
        assert_eq!(
            with_fake_curl("printf '{\"status\":\"up\"}\n200'", "https://app.berth"),
            Some(200)
        );
    }

    #[test]
    fn run_curl_command_with_body_keeps_body_and_status() {
        let response =
            with_fake_curl_output("printf '{\"status\":\"up\"}\n200'", "https://app.berth")
                .expect("curl output");
        assert!(response.status.success());

        let stdout = String::from_utf8_lossy(&response.stdout).to_string();
        assert!(stdout.contains("{\"status\":\"up\"}"));
        assert!(stdout.trim_end().ends_with("200"));
    }
}
