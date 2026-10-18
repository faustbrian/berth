//! The shell that seeds a Redis or Valkey server's ACL file.

/// Returns the `sh -c` script that installs stdin as the ACL file at `path`
/// unless that file already gives the `default` (admin) user a password
/// hash, in which case stdin is read and discarded.
///
/// - Keeping such a file lets a server recreated on its kept volume
///   (rootless Podman's network repair) keep the tenant users `ACL SAVE`
///   wrote into it.
/// - Anything else is replaced: an empty or partial file from an
///   interrupted seed would otherwise start Redis with a `default` user
///   that needs no password.
/// - The new file is written beside the old one, checked the same way and
///   only then renamed over it, so an interrupted seed never installs a
///   partial ACL file.
pub(super) fn acl_seed_script(path: &str) -> String {
    // A complete admin line: password hash, then its command rules.
    let valid = r"'^user default .*#[0-9a-f]{64} .*\+@all'";
    format!(
        "if grep -Eq {valid} {path} 2>/dev/null; then cat > /dev/null; \
         else cat > {path}.next && grep -Eq {valid} {path}.next && mv {path}.next {path}; fi"
    )
}

#[cfg(test)]
mod tests {
    use super::acl_seed_script;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    const SEED: &str = "user default on #0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef ~* &* +@all\n";

    fn seed(path: &std::path::Path) {
        assert!(seed_with(path, SEED), "the seed succeeds");
    }

    fn seed_with(path: &std::path::Path, input: &str) -> bool {
        let mut child = Command::new("sh")
            .args(["-c", &acl_seed_script(&path.display().to_string())])
            .stdin(Stdio::piped())
            .spawn()
            .expect("sh");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write");
        child.wait().expect("wait").success()
    }

    #[test]
    fn keeps_a_saved_acl_file_and_replaces_anything_without_an_admin_password() {
        let dir = std::env::temp_dir().join(format!("berth-acl-seed-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("users.acl");
        let read = || std::fs::read_to_string(&file).expect("acl");

        seed(&file);
        assert_eq!(read(), SEED, "a new volume is seeded");
        assert!(!dir.join("users.acl.next").exists());

        // What `ACL SAVE` writes after a tenant was added.
        let saved = format!(
            "user default on sanitize-payload #{} ~* &* +@all\nuser tenant on #{} ~t:* -@all\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        std::fs::write(&file, &saved).expect("saved acl");
        seed(&file);
        assert_eq!(read(), saved, "tenant users survive a recreated server");

        let truncated = format!("user default on #{}", "c".repeat(64));
        for broken in [
            "",
            "user default on ~* &* +@all\n",
            "user default on #01",
            truncated.as_str(),
        ] {
            std::fs::write(&file, broken).expect("broken acl");
            seed(&file);
            assert_eq!(read(), SEED, "{broken:?} is replaced");
        }

        // A seed cut short fails and never installs the partial file.
        std::fs::remove_file(&file).expect("fresh volume");
        assert!(!seed_with(&file, &truncated), "a truncated seed fails");
        assert!(!file.exists(), "no partial ACL file is installed");
        drop(std::fs::remove_dir_all(&dir));
    }
}
