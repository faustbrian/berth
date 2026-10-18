//! cli support open in browser module.
//!
//! Contains cli support open in browser logic used by Berth command workflows.

use std::process::Command;

#[cfg(test)]
thread_local! {
    static FAKE_OPEN_COMMAND: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn open_command() -> String {
    #[cfg(test)]
    {
        FAKE_OPEN_COMMAND.with(|command| {
            command
                .borrow()
                .clone()
                .unwrap_or_else(default_open_command)
        })
    }
    #[cfg(not(test))]
    {
        default_open_command()
    }
}

fn default_open_command() -> String {
    if cfg!(target_os = "macos") {
        "open".to_owned()
    } else {
        "xdg-open".to_owned()
    }
}

pub(crate) fn open_in_browser(url: &str) {
    let command = open_command();
    open_in_browser_with_command(url, &command);
}

fn open_in_browser_with_command(url: &str, command: &str) {
    drop(Command::new(command).arg(url).status());
}

#[cfg(test)]
pub(crate) fn with_open_command<F, T>(command: &str, test: F) -> T
where
    F: FnOnce() -> T,
{
    let previous = FAKE_OPEN_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        let previous = current.clone();
        *current = Some(command.to_owned());
        previous
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    FAKE_OPEN_COMMAND.with(|stored| {
        let mut current = stored.borrow_mut();
        *current = previous;
    });

    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

#[cfg(test)]
mod tests {
    use super::open_in_browser_with_command;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_fake_binary<F: FnOnce(&Path, &str)>(name: &str, body: F) {
        let base = crate::test_support::temp_root();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("time");
        let bin_dir = base.join(format!("berth-open-browser-{}", stamp.as_nanos()));
        fs::create_dir_all(&bin_dir).expect("create fake bin dir");

        let fake = bin_dir.join(name);
        let mut file = fs::File::create(&fake).expect("create fake command");
        writeln!(
            file,
            "#!/bin/sh\nprintf \"%s\\n\" \"$1\" > \"{}/invoked\"\n",
            bin_dir.display()
        )
        .expect("write fake command");
        let mut perms = fs::metadata(&fake).expect("metadata").permissions();
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&fake, perms).expect("set mode");
        }

        let fake_command = fake.to_string_lossy().to_string();
        body(&bin_dir, &fake_command);
        drop(fs::remove_dir_all(&bin_dir));
    }

    #[test]
    fn open_in_browser_invokes_platform_binary() {
        let command = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };

        with_fake_binary(command, |bin_dir, binary| {
            open_in_browser_with_command("https://example.internal", binary);

            let marker = bin_dir.join("invoked");
            let contents = fs::read_to_string(marker).expect("command invoked");
            assert_eq!(contents, "https://example.internal\n");
        });
    }

    #[test]
    fn open_in_browser_drops_errors_when_binary_missing() {
        open_in_browser_with_command("https://example.internal", "/tmp/does-not-exist");
    }
}
