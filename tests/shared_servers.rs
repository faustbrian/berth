//! Shared `PostgreSQL`, `MySQL` and `MariaDB` servers through the real `berth`
//! binary against a fake Docker: one server per compatible image, one tenant
//! per instance, and lifecycle commands that never touch the server other
//! instances use.

// Integration tests are their own crate: panicking on setup failures and
// top-level `#[test]` functions are the idiom here.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::tests_outside_test_module
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Records every call in `log`, the environment's `POSTGRES_PASSWORD` in
/// `run-env-*`, and every SQL script sent on stdin in `sql`. Containers carry
/// their labels; network attachments live in `att/{container}/{network}`
/// (one alias per line). `FAKE_NAMES` lists the databases and roles a server
/// holds; `FAKE_OWNED` lists `database:owner` pairs on top.
const FAKE_DOCKER: &str = r#"#!/bin/sh
# The gateway reload sends its config on stdin; take it all.
case "$*" in *"caddy reload"*) cat > /dev/null ;; esac
S="$FAKE_STATE"
echo "$*" >> "$S/log"
case "$1 $2" in
  "network inspect")
    for arg in "$@"; do net="$arg"; done
    [ -f "$S/n-$net" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create")
    : > "$S/nl"; prev=""
    for arg in "$@"; do [ "$prev" = "--label" ] && echo "$arg" >> "$S/nl"; prev="$arg"; last="$arg"; done
    mv "$S/nl" "$S/n-$last"; exit 0 ;;
  "network ls")
    for f in "$S"/n-*; do
      [ -f "$f" ] || continue
      keep=1; prev=""
      for arg in "$@"; do
        if [ "$prev" = "--filter" ]; then
          case "$arg" in label=*) grep -qx "${arg#label=}" "$f" || keep=0 ;; esac
        fi
        prev="$arg"
      done
      [ $keep = 1 ] && basename "$f" | sed 's/^n-//'
    done
    exit 0 ;;
  "network rm")
    for f in "$S"/att/*/"$3"; do [ -f "$f" ] && { echo "Error: network $3 has active endpoints" >&2; exit 1; }; done
    rm -f "$S/n-$3"; exit 0 ;;
  "network connect")
    shift 2; aliases=""; prev=""; rest=""
    for arg in "$@"; do
      if [ "$prev" = "--alias" ]; then aliases="$aliases $arg"; elif [ "$arg" != "--alias" ]; then rest="$rest $arg"; fi
      prev="$arg"
    done
    set -- $rest; net="$1"; ctr="$2"
    # `connect-hold`: the reconnect of an alias change waits (between its
    # disconnect and connect) until the file goes.
    [ -f "$S/connect-hold" ] && { : > "$S/connect-waiting"; while [ -f "$S/connect-hold" ]; do sleep 0.05; done; }
    [ -f "$S/att/$ctr/$net" ] && { echo "Error: endpoint with name $ctr already exists in network $net" >&2; exit 1; }
    mkdir -p "$S/att/$ctr"; : > "$S/att/$ctr/$net"
    for a in $aliases; do echo "$a" >> "$S/att/$ctr/$net"; done
    exit 0 ;;
  "network disconnect")
    for arg in "$@"; do last2="$last"; last="$arg"; done
    [ -f "$S/att/$last/$last2" ] || { echo "Error: container $last is not connected to network $last2" >&2; exit 1; }
    rm -f "$S/att/$last/$last2"; exit 0 ;;
  "image inspect") exit 0 ;;
  "volume create"|"volume rm"|"volume ls") exit 0 ;;
esac
case "$1" in
  run)
    case "$*" in *"--entrypoint printenv"*)
      [ -n "$FAKE_PROBE_FAIL" ] && { echo "docker: Error response from daemon: NotFound: parent snapshot does not exist" >&2; exit 125; }
      echo /var/lib/postgresql/data; exit 0 ;; esac
    case "$*" in *"--entrypoint sh"*) cat > "$S/acl-seed"; exit 0 ;; esac
    # A throwaway client: records its stdin and whether a password reached
    # its environment, then exits as asked.
    case "$*" in *" --rm "*)
      cat > "$S/client-stdin"
      # FAKE_TENANT_LOGIN_FAILS: the tenant's own login is refused.
      [ -n "$FAKE_TENANT_LOGIN_FAILS" ] && { echo 'psql: error: connection to server at "db" (172.18.0.2), port 5432 failed: FATAL:  password authentication failed for user "tenant"' >&2; exit 2; }
      for var in PGPASSWORD MYSQL_PWD REDISCLI_AUTH; do
        eval "value=\${$var:-}"; [ -n "$value" ] && echo "$var" >> "$S/client-env"
      done
      # A dump prints FAKE_DUMP_OUT before it exits as asked; a restore's
      # psql prints berth's markers (`docker/exec/psql_restore.rs`).
      [ -n "$FAKE_DUMP_OUT" ] && printf '%s' "$FAKE_DUMP_OUT"
      # FAKE_SWITCHED: the dump ran on into another connection (`\\c`).
      # A client echoes berth's token after the dump (and MySQL's database).
      tok=$(grep -v '^[[:space:]]*$' "$S/client-stdin" | tail -n 2 | sed -n 's/.*berth-dump-end \([0-9a-f][0-9a-f]*\).*/\1/p' | tail -n 1); [ -n "$tok" ] && echo "berth-dump-end $tok"; grep -q 'berth-db' "$S/client-stdin" && { for a in "$@"; do db="$a"; done; echo "berth-db $db"; }
      case "$*" in *berth-backend-end*) [ -z "$FAKE_OLD_SERVER" ] && printf ' berth-backend-start 7\n'; [ "$FAKE_OLD_SERVER" = 9 ] && echo 'psql:<stdin>:1: ERROR:  function pg_catalog.txid_current_if_assigned() does not exist' >&2; printf ' berth-backend-end %s true\nberth-rollback on\n' "${FAKE_SWITCHED:-7}" ;; esac
      exit "${FAKE_CLIENT_EXIT:-0}" ;;
    esac
    name=""; prev=""; : > "$S/labels"
    net=""
    for arg in "$@"; do
      [ "$prev" = "--name" ] && name="$arg"
      [ "$prev" = "--label" ] && echo "$arg" >> "$S/labels"
      [ "$prev" = "--network" ] && net="$arg"
      prev="$arg"
    done
    mkdir -p "$S/c"; mv "$S/labels" "$S/c/$name"; echo "$prev" > "$S/img-$name"
    [ -n "$net" ] && { mkdir -p "$S/att/$name"; : > "$S/att/$name/$net"; }
    [ -n "$POSTGRES_PASSWORD" ] && echo "set" > "$S/run-env-$name"
    exit 0 ;;
  rm|stop|start|kill|pause|unpause)
    # FAKE_RM_DIAL: the engine went away (its socket is gone).
    [ "$1" = rm ] && [ -n "$FAKE_RM_DIAL" ] && { echo 'failed to connect to the docker API at unix:///x/docker.sock: dial unix /x/docker.sock: connect: no such file or directory' >&2; exit 1; }
    [ "$1" = rm ] && for arg in "$@"; do rm -f "$S/c/$arg"; rm -rf "$S/att/$arg"; done
    [ "$1" = start ] && for arg in "$@"; do rm -f "$S/stopped-$arg"; done
    exit 0 ;;
  ps)
    case "$*" in
      *network=*"label=dev.berth.managed=true"*)
        net=$(printf '%s\n' "$@" | sed -n 's/^network=//p')
        for f in "$S"/att/*/"$net"; do [ -f "$f" ] && basename "$(dirname "$f")"; done ;;
      *"label=dev.berth.role=shared-server"*)
        for f in "$S"/c/berth_shared_*; do
          [ -f "$f" ] || continue
          fp=$(grep '^dev.berth.shared.fingerprint=' "$f" | cut -d= -f2)
          engine=$(grep '^dev.berth.shared.engine=' "$f" | cut -d= -f2)
          printf '%s\t%s\t%s\t%s\n' "$(basename "$f")" "$engine" "$fp" "$(cat "$S/img-$(basename "$f")")"
        done ;;
    esac
    exit 0 ;;
  exec)
    case "$*" in *"'php' 'artisan' 'test'"*) [ -n "$FAKE_TEST_SLEEP" ] && sleep "$FAKE_TEST_SLEEP"; exit 0 ;; esac
    cat > "$S/last-sql"; cat "$S/last-sql" >> "$S/sql"
    tok=$(grep -v '^[[:space:]]*$' "$S/last-sql" | tail -n 2 | sed -n 's/.*berth-dump-end \([0-9a-f][0-9a-f]*\).*/\1/p' | tail -n 1); [ -n "$tok" ] && echo "berth-dump-end $tok"; grep -q 'berth-db' "$S/last-sql" && { for a in "$@"; do db="$a"; done; echo "berth-db $db"; }
    case "$*" in *berth-backend-end*) [ -z "$FAKE_OLD_SERVER" ] && printf ' berth-backend-start 7\n'; [ "$FAKE_OLD_SERVER" = 9 ] && echo 'psql:<stdin>:1: ERROR:  function pg_catalog.txid_current_if_assigned() does not exist' >&2; printf ' berth-backend-end %s true\nberth-rollback on\n' "${FAKE_SWITCHED:-7}" ;; esac
    # FAKE_CLIENT_FATAL: the database refuses the restore's client (a role
    # that may not set the English messages it asks for).
    # FAKE_CLIENT_NO_DB: the client's database (its last argument) is gone.
    # FAKE_CLIENT_NO_DB=de: the server says so in German; its catalog,
    # asked through the `postgres` database, says the same.
    for a in "$@"; do db="$a"; done
    # FAKE_NO_MAINTENANCE: the server's `postgres` database refuses too.
    if [ -n "$FAKE_NO_MAINTENANCE" ] && [ "$db" = postgres ]; then
      echo 'psql: error: connection to server on socket "/var/run/postgresql/.s.PGSQL.5432" failed: FATAL:  database "postgres" does not exist' >&2; exit 2
    fi
    if [ -n "$FAKE_CLIENT_NO_DB" ] && [ "$db" = postgres ] && grep -q pg_database "$S/last-sql"; then
      printf '   ?column?\n---------------\n berth: absent\n(1 row)\n\n'; exit 0
    fi
    if [ -n "$FAKE_CLIENT_NO_DB" ] && grep -qx 'SELECT 1;' "$S/last-sql"; then
      [ "$FAKE_CLIENT_NO_DB" = de ] && { echo "psql: Fehler: Verbindung zum Server auf Socket »/var/run/postgresql/.s.PGSQL.5432« fehlgeschlagen: FATAL:  Datenbank »${db}« existiert nicht" >&2; exit 2; }
      echo "psql: error: connection to server on socket \"/var/run/postgresql/.s.PGSQL.5432\" failed: FATAL:  database \"$db\" does not exist" >&2; exit 2
    fi
    [ -n "$FAKE_CLIENT_FATAL" ] && grep -qx 'SELECT 1;' "$S/last-sql" && { echo 'psql: error: connection to server on socket "/var/run/postgresql/.s.PGSQL.5432" failed: FATAL:  permission denied to set parameter "lc_messages"' >&2; exit 2; }
    # FAKE_SERVER=exits stops the shared server on its first admin query,
    # FAKE_SERVER=badauth rejects the admin password, FAKE_SERVER=norole
    # answers with a permanent connection error.
    case "$FAKE_SERVER:$*" in
      refusing:*psql*)
        [ -f "$S/refuse" ] && { echo 'psql: error: connection to server at "127.0.0.1", port 5432 failed: Connection refused' >&2; exit 2; } ;;
      norole:*psql*)
        echo 'psql: error: connection to server at "127.0.0.1", port 5432 failed: FATAL:  role "berth_admin" does not exist' >&2; exit 2 ;;
      exits:*psql*)
        for f in "$S"/c/berth_shared_*; do touch "$S/stopped-$(basename "$f")"; done
        echo 'psql: error: connection to server at "127.0.0.1", port 5432 failed: Connection refused' >&2; exit 2 ;;
      badauth:*psql*)
        echo 'psql: error: connection to server at "127.0.0.1", port 5432 failed: FATAL:  password authentication failed for user "berth_admin"' >&2; exit 2 ;;
    esac
    [ -f "$S/tenant-fails" ] && grep -q 'CREATE ROLE' "$S/last-sql" && { echo 'ERROR:  permission denied to create role' >&2; exit 1; }
    case "$*" in *psql*|*" mysql "*|*" mariadb "*|*redis-cli*) ;; *) exit 0 ;; esac
    case "$*" in
      # The one-exec scan of every database: `database<TAB>key` lines.
      *"sh -c"*"--scan"*)
        for k in $FAKE_KEYS; do printf '0\t%s\n' "$k"; done
        [ -f "$S/keys-db1" ] && while IFS= read -r k; do printf '1\t%s\n' "$k"; done < "$S/keys-db1"
        echo scan >> "$S/scans"; exit 0 ;;
      *"--scan"*) exit 0 ;;
    esac
    tail -n 1 "$S/sql" | grep -q '^ACL USERS' && { for n in $FAKE_NAMES; do echo "$n"; done; exit 0; }
    tail -n 1 "$S/sql" | grep -q '^CONFIG GET databases' && { printf 'databases\n16\n'; exit 0; }
    tail -n 1 "$S/sql" | grep -q '^INFO server' && { printf '# Server\nredis_version:7.2.4\n'; exit 0; }
    tail -n 3 "$S/sql" | grep -q 'server_version_num' && { echo 170004; exit 0; }
    # MySQL's listing has no database owners (`list_names_sql`).
    if tail -n 3 "$S/sql" | grep -q 'FROM mysql.user'; then
      for n in $FAKE_NAMES; do printf 'd\t%s\t\nr\t%s\t\n' "$n" "$n"; done
      exit 0
    fi
    if tail -n 3 "$S/sql" | grep -q 'FROM pg_roles'; then
      for n in $FAKE_NAMES; do printf 'd\t%s\tberth_admin\nr\t%s\t\n' "$n" "$n"; done
      for pair in $FAKE_OWNED; do printf 'd\t%s\t%s\n' "${pair%%:*}" "${pair#*:}"; done
      exit 0
    fi
    exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    # `inspect-fails-<name>`: the engine does not answer for that container.
    [ -f "$S/inspect-fails-$name" ] && { echo "Error response from daemon: context deadline exceeded" >&2; exit 1; }
    [ -f "$S/c/$name" ] || { echo "Error: No such object: $name" >&2; exit 1; }
    case "$*" in
      *NetworkSettings.Networks*)
        printf '{'; sep=""
        for f in "$S/att/$name"/*; do
          [ -f "$f" ] || continue
          printf '%s"%s":{"Aliases":[' "$sep" "$(basename "$f")"; asep=""
          while read -r a; do printf '%s"%s"' "$asep" "$a"; asep=","; done < "$f"
          printf '%s"%s","0123456789ab"]}' "$asep" "$name"; sep=","
        done
        printf '}' ;;
      *dev.berth.shared.fingerprint*)
        installation=$(grep '^dev.berth.installation=' "$S/c/$name" | cut -d= -f2)
        fp=$(grep '^dev.berth.shared.fingerprint=' "$S/c/$name" | cut -d= -f2)
        state=running; [ -f "$S/stopped-$name" ] && state=exited
        printf '%s\t%s\t%s' "$state" "$installation" "$fp" ;;
      *dev.berth.installation*) cat "$BERTH_HOME/installation-id" ;;
      *--format*) if [ -f "$S/stopped-$name" ]; then printf exited; else printf running; fi ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

const PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n  app:\n    preset: laravel\n";

/// One fake Docker and berth home, shared by every checkout in a test.
/// Two free loopback ports for this fixture's gateway, so parallel test
/// binaries (and a real gateway on the developer's machine) never contend
/// for the default 8443/8080. They come from 20000–39999, below the OS
/// ephemeral range, and each is claimed with an exclusive marker file, so
/// fixtures in other test processes never pick the same port (berth binds
/// it briefly to check that it is free). [`release_ports`] drops the claims.
fn free_ports() -> (u16, u16) {
    let claims = std::env::temp_dir().join("berth-test-port-claims");
    std::fs::create_dir_all(&claims).expect("claims dir");
    let mut port = 20_000 + u16::try_from(std::process::id() % 20_000).expect("below 20000");
    let mut pick = || loop {
        port = 20_000 + (port - 20_000 + 7_919) % 20_000;
        let claimed = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(claims.join(port.to_string()))
            .is_ok();
        // A connect probe, not a bind: macOS sets close-on-exec on a new socket
        // only after creating it, so a listener bound here can leak into a
        // `berth` another test thread spawns at that instant and keep the
        // port bound, failing that fixture's gateway with "port in use".
        // A refused connection means nothing listens there.
        if claimed && std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            return port;
        }
    };
    let https = pick();
    (https, pick())
}

fn release_ports(ports: (u16, u16)) {
    let claims = std::env::temp_dir().join("berth-test-port-claims");
    for port in [ports.0, ports.1] {
        drop(std::fs::remove_file(claims.join(port.to_string())));
    }
}

struct Host {
    base: PathBuf,
    ports: (u16, u16),
}

impl Host {
    fn new(label: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("berth-shared-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["bin", "state/c", "home", "user-home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        // The same fake answers as `podman`, for runs with BERTH_ENGINE=podman.
        for engine in ["docker", "podman"] {
            let path = base.join("bin").join(engine);
            std::fs::write(&path, FAKE_DOCKER).expect("fake engine");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        Self {
            base,
            ports: free_ports(),
        }
    }

    /// A checkout `name` holding `project` as its `.berth.yaml`.
    fn checkout(&self, name: &str, project: &str) -> PathBuf {
        let dir = self.base.join(name);
        std::fs::create_dir_all(&dir).expect("checkout");
        std::fs::write(dir.join(".berth.yaml"), project).expect("project file");
        std::fs::write(dir.join("artisan"), "#!/usr/bin/env php\n").expect("artisan");
        dir
    }

    fn berth(&self, checkout: &Path, args: &[&str]) -> Output {
        self.berth_with(checkout, args, "")
    }

    fn berth_with(&self, checkout: &Path, args: &[&str], names: &str) -> Output {
        self.berth_with_owned(checkout, args, names, "")
    }

    fn berth_with_owned(&self, checkout: &Path, args: &[&str], names: &str, owned: &str) -> Output {
        bounded_output(&mut self.command(checkout, args, names, owned))
    }

    fn command(&self, checkout: &Path, args: &[&str], names: &str, owned: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
        command
            .current_dir(checkout)
            .args(args)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.base.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("HOME", self.base.join("user-home"))
            .env("BERTH_HOME", self.base.join("home"))
            .env("BERTH_GATEWAY_HTTPS_PORT", self.ports.0.to_string())
            .env("BERTH_GATEWAY_HTTP_PORT", self.ports.1.to_string())
            .env("FAKE_STATE", self.base.join("state"))
            .env("FAKE_NAMES", names)
            .env("FAKE_OWNED", owned)
            .env("FAKE_KEYS", names)
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_ENGINE")
            .stdin(std::process::Stdio::null());
        command
    }

    /// Runs berth and fails the test with its stderr unless it succeeded.
    fn ok(&self, checkout: &Path, args: &[&str]) -> Output {
        let output = self.berth(checkout, args);
        assert!(
            output.status.success(),
            "{args:?} failed: {}",
            text(&output.stderr)
        );
        output
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.base.join("state/log")).unwrap_or_default()
    }

    fn sql(&self) -> String {
        std::fs::read_to_string(self.base.join("state/sql")).unwrap_or_default()
    }

    fn clear(&self) {
        drop(std::fs::remove_file(self.base.join("state/log")));
        drop(std::fs::remove_file(self.base.join("state/sql")));
    }

    fn servers(&self) -> Vec<String> {
        std::fs::read_dir(self.base.join("state/c"))
            .expect("containers")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("berth_shared_postgres_"))
            .collect()
    }

    /// Every secret berth created under its home.
    fn secrets(&self) -> Vec<String> {
        let mut secrets = Vec::new();
        let mut stack = vec![self.base.join("home")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)
                .expect("home")
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.parent().is_some_and(|parent| {
                    parent.ends_with("credentials")
                        || parent.parent().is_some_and(|p| p.ends_with("shared"))
                }) {
                    secrets.push(std::fs::read_to_string(&path).expect("secret"));
                }
            }
        }
        secrets
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        release_ports(self.ports);
        drop(std::fs::remove_dir_all(&self.base));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn env_json(host: &Host, checkout: &Path) -> serde_json::Value {
    let output = host.berth(checkout, &["env", "--json"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("env JSON")
}

#[test]
fn instances_share_one_server_with_a_closed_tenant_each_and_no_secret_in_argv() {
    let host = Host::new("share");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    for checkout in [&a, &b] {
        let up = host.berth(checkout, &["up", "--no-wait"]);
        assert!(up.status.success(), "{}", text(&up.stderr));
    }
    assert_eq!(host.servers().len(), 1, "one server for both instances");
    let calls = host.calls();
    let runs = calls
        .lines()
        .filter(|line| line.starts_with("run ") && line.contains("berth_shared_postgres_"))
        .count();
    assert_eq!(runs, 1, "{calls}");
    assert!(
        calls
            .lines()
            .filter(|line| line.starts_with("network connect --alias db "))
            .count()
            >= 2,
        "the server answers as `db` on each instance network: {calls}"
    );

    let env_a = env_json(&host, &a);
    let env_b = env_json(&host, &b);
    let tenant_a = env_a["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let tenant_b = env_b["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    assert!(
        tenant_a.starts_with("shop__a-") && tenant_a.ends_with("__db"),
        "{tenant_a}"
    );
    assert_ne!(tenant_a, tenant_b);
    assert_eq!(env_a["environment"]["DB_USERNAME"], tenant_a.as_str());
    assert_eq!(env_a["environment"]["DB_HOST"], "db");
    assert_ne!(
        env_a["environment"]["DB_PASSWORD"],
        env_b["environment"]["DB_PASSWORD"]
    );
    assert!(
        env_a["host"].get("DB_HOST").is_none(),
        "a tenant has no host port"
    );

    let sql = host.sql();
    assert!(
        sql.contains(&format!(
            "CREATE ROLE \"{tenant_a}\" LOGIN CREATEDB NOSUPERUSER"
        )),
        "{sql}"
    );
    assert!(
        sql.contains(&format!(
            "REVOKE ALL ON DATABASE \"{tenant_b}\" FROM PUBLIC"
        )),
        "{sql}"
    );
}

#[test]
fn secrets_stay_private_and_out_of_every_docker_argument() {
    let host = Host::new("secrets");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    for checkout in [&a, &b] {
        let up = host.berth(checkout, &["up", "--no-wait"]);
        assert!(up.status.success(), "{}", text(&up.stderr));
    }
    let calls = host.calls();
    let tenant_a = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let secrets = host.secrets();
    assert_eq!(
        secrets.len(),
        3,
        "an admin secret and a password per tenant"
    );
    for secret in &secrets {
        assert_eq!(secret.len(), 64);
        assert!(
            !calls.contains(secret.as_str()),
            "a secret reached a docker argument"
        );
    }
    let run_env = std::fs::read_dir(host.base.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("run-env-berth_shared_postgres_")
        });
    assert!(
        run_env,
        "the admin secret reaches the server through the environment"
    );
    let mode = std::fs::metadata(
        host.base
            .join("home/instances/shop")
            .join(
                tenant_a
                    .trim_start_matches("shop__")
                    .trim_end_matches("__db"),
            )
            .join("credentials")
            .join(&tenant_a),
    )
    .expect("credentials")
    .permissions()
    .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn down_stop_and_rm_leave_the_server_running_for_other_instances() {
    let host = Host::new("down");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    for args in [&["stop"][..], &["rm", "--force"][..], &["down"][..]] {
        host.clear();
        let output = host.berth(&a, args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
        let calls = host.calls();
        for line in calls.lines() {
            let touches_server = line.split(' ').any(|word| word == server);
            let allowed = line.starts_with("network disconnect") || line.starts_with("inspect");
            assert!(
                !touches_server || allowed,
                "{args:?} touched the server: {line}"
            );
        }
    }
    assert_eq!(host.servers(), [server], "the server still exists");
    let status = host.berth(&a, &["ps", "--format", "json"]);
    assert!(status.status.success(), "{}", text(&status.stderr));
    assert!(
        text(&status.stdout).contains("\"exited\""),
        "a detached tenant is not reported running: {}",
        text(&status.stdout)
    );
}

#[test]
fn purge_drops_only_the_instance_tenants_and_their_test_databases() {
    let host = Host::new("purge");
    let a = host.checkout("a", PROJECT);
    let longer = host.checkout("a-b", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&longer, &["up", "--no-wait"]);
    let mine = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let theirs = env_json(&host, &longer)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let base = mine.trim_end_matches("__db").to_owned();
    let names = format!(
        "postgres template1 berth_admin {mine} {mine}_test_1 {mine}_test_2 {base}__db__staging {theirs} {theirs}_test_1"
    );
    host.clear();
    let purge = host.berth_with(&a, &["down", "--purge"], &names);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let sql = host.sql();
    for dropped in [
        &mine,
        &format!("{mine}_test_1"),
        &format!("{mine}_test_2"),
        &format!("{base}__db__staging"),
    ] {
        assert!(
            sql.contains(&format!(
                "DROP DATABASE IF EXISTS \"{dropped}\" WITH (FORCE)"
            )),
            "{dropped}: {sql}"
        );
    }
    assert!(
        sql.contains(&format!("DROP ROLE IF EXISTS \"{mine}\"")),
        "{sql}"
    );
    for kept in ["postgres", "template1", "berth_admin", theirs.as_str()] {
        assert!(
            !sql.contains(&format!("\"{kept}\"")),
            "{kept} must survive: {sql}"
        );
    }
    assert!(!sql.contains(&format!("\"{theirs}_test_1\"")), "{sql}");
    assert!(
        !host
            .base
            .join("home/instances/shop")
            .join(base.trim_start_matches("shop__"))
            .exists()
    );
    assert_eq!(
        host.servers().len(),
        1,
        "purge leaves the shared server to `berth gc`"
    );
}

#[test]
fn a_runtime_environment_gets_its_own_tenant() {
    let host = Host::new("env");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let staging = host.berth(&a, &["up", "--no-wait", "--env", "staging"]);
    assert!(staging.status.success(), "{}", text(&staging.stderr));
    let output = host.berth(&a, &["env", "--json", "--env", "staging"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).expect("env JSON");
    let tenant = document["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant");
    assert!(tenant.ends_with("__db__staging"), "{tenant}");
    assert_eq!(host.servers().len(), 1, "the same server");
}

#[test]
fn what_a_tenant_cannot_have_fails_before_docker_is_touched() {
    let cases = [
        (
            "publish",
            "    publish: true\n",
            "cannot publish a host port",
        ),
        (
            "password",
            "    password: secret123\n",
            "generated credentials",
        ),
        ("database", "    database: mine\n", "generated credentials"),
        ("volumes", "    volumes: [\"./dump:/dump\"]\n", "`volumes`"),
        ("env", "    env:\n      TZ: UTC\n", "`env`"),
        ("port", "    port: 5433\n", "`port`"),
        // Run in the server, a hook would act on every instance's data.
        (
            "hook",
            "    hook:\n      - name: peek\n        phase: post_up\n        run:\n          type: exec\n          argv: [echo, HOOK-SENTINEL]\n",
            "`hook`",
        ),
    ];
    for (label, extra, message) in cases {
        let host = Host::new(&format!("reject-{label}"));
        let project = PROJECT.replace(
            "    preset: postgres\n",
            &format!("    preset: postgres\n{extra}"),
        );
        let a = host.checkout("a", &project);
        let up = host.berth(&a, &["up", "--no-wait"]);
        assert_eq!(up.status.code(), Some(125), "{label}");
        let stderr = text(&up.stderr);
        assert!(
            stderr.contains(message) && stderr.contains("isolation: dedicated"),
            "{label}: {stderr}"
        );
        assert!(!host.calls().contains("run "), "{label}: {}", host.calls());
        assert!(
            !host.calls().contains("HOOK-SENTINEL"),
            "{label}: {}",
            host.calls()
        );
    }
    let host = Host::new("reject-memcached");
    let a = host.checkout(
        "a",
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  cache:\n    preset: memcached\n    isolation: shared\n",
    );
    let up = host.berth(&a, &["up", "--no-wait"]);
    assert_eq!(up.status.code(), Some(125));
    assert!(
        text(&up.stderr).contains("only for PostgreSQL"),
        "{}",
        text(&up.stderr)
    );
}

#[test]
fn dedicated_isolation_keeps_a_container_of_the_instance_own() {
    let host = Host::new("dedicated");
    let project = PROJECT.replace(
        "    preset: postgres\n",
        "    preset: postgres\n    isolation: dedicated\n    password: secret123\n",
    );
    let a = host.checkout("a", &project);
    let up = host.berth(&a, &["up", "--no-wait"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(host.servers().is_empty());
    assert!(
        !host.calls().contains("secret123"),
        "a dedicated database's password stays out of argv"
    );
    assert!(
        host.calls()
            .lines()
            .any(|line| line.starts_with("run ") && line.contains("--name berth_shop_a-")),
        "{}",
        host.calls()
    );
}

#[test]
fn a_dedicated_database_that_refuses_the_client_is_never_reset() {
    let host = Host::new("dedicated-refuses");
    let project = PROJECT.replace(
        "    preset: postgres\n",
        "    preset: postgres\n    isolation: dedicated\n    password: secret123\n",
    );
    let a = host.checkout("a", &project);
    host.ok(&a, &["up", "--no-wait"]);
    std::fs::write(a.join("dump.sql"), "CREATE TABLE users (id int);\n").expect("dump");
    host.clear();
    let restore = bounded_output(
        host.command(
            &a,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_CLIENT_FATAL", "1"),
    );
    let stderr = text(&restore.stderr);
    assert!(!restore.status.success(), "{stderr}");
    assert!(
        stderr.contains("refuses berth's database client, so nothing was changed")
            && stderr.contains("permission denied to set parameter")
            && stderr.contains("to be a superuser"),
        "{stderr}"
    );
    // A dedicated reset runs as `psql -c` arguments: the call log has it.
    let calls = format!("{}{}", host.calls(), host.sql());
    assert!(!calls.contains("DROP DATABASE"), "{calls}");
}

#[test]
fn a_reset_restore_recreates_a_dedicated_database_that_is_gone() {
    let host = Host::new("dedicated-no-db");
    let project = PROJECT.replace(
        "    preset: postgres\n",
        "    preset: postgres\n    isolation: dedicated\n    password: secret123\n",
    );
    let a = host.checkout("a", &project);
    host.ok(&a, &["up", "--no-wait"]);
    std::fs::write(a.join("dump.sql"), "CREATE TABLE users (id int);\n").expect("dump");
    let restore = |args: &[&str]| {
        host.clear();
        bounded_output(host.command(&a, args, "", "").env("FAKE_CLIENT_NO_DB", "1"))
    };
    // Without `--reset` nothing would create it: refused as before.
    let plain = restore(&["restore", "--service", "db", "--file", "dump.sql"]);
    assert!(!plain.status.success(), "{}", text(&plain.stderr));
    assert!(
        text(&plain.stderr).contains("does not exist"),
        "{}",
        text(&plain.stderr)
    );
    // `--reset` creates it, so the missing database is no reason to refuse.
    let reset = restore(&[
        "restore",
        "--service",
        "db",
        "--reset",
        "--file",
        "dump.sql",
    ]);
    assert!(reset.status.success(), "{}", text(&reset.stderr));
    let calls = format!("{}{}", host.calls(), host.sql());
    assert!(calls.contains("CREATE DATABASE"), "{calls}");
    // A server answering in another language: its catalog says it is gone.
    host.clear();
    let german = bounded_output(
        host.command(
            &a,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_CLIENT_NO_DB", "de"),
    );
    assert!(german.status.success(), "{}", text(&german.stderr));
}

#[test]
fn a_reset_into_a_missing_database_on_an_old_server_creates_nothing() {
    // The service's database is gone, so the version is asked of the
    // server's `postgres` database before the reset creates it.
    let host = Host::new("dedicated-no-db-old");
    let project = PROJECT.replace(
        "    preset: postgres\n",
        "    preset: postgres\n    isolation: dedicated\n    password: secret123\n",
    );
    let a = host.checkout("a", &project);
    host.ok(&a, &["up", "--no-wait"]);
    std::fs::write(a.join("dump.sql"), "CREATE TABLE users (id int);\n").expect("dump");
    host.clear();
    let reset = bounded_output(
        host.command(
            &a,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_CLIENT_NO_DB", "1")
        .env("FAKE_OLD_SERVER", "1"),
    );
    assert!(!reset.status.success());
    assert!(
        text(&reset.stderr).contains("older than 10.7"),
        "{}",
        text(&reset.stderr)
    );
    let calls = format!("{}{}", host.calls(), host.sql());
    assert!(
        !calls.contains("CREATE DATABASE") && !calls.contains("DROP"),
        "{calls}"
    );
    // Its `postgres` database cannot be asked either: refused all the same.
    host.clear();
    let unasked = bounded_output(
        host.command(
            &a,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_CLIENT_NO_DB", "1")
        .env("FAKE_NO_MAINTENANCE", "1"),
    );
    assert!(!unasked.status.success());
    assert!(
        text(&unasked.stderr).contains("could not ask its `postgres` database"),
        "{}",
        text(&unasked.stderr)
    );
    let calls = format!("{}{}", host.calls(), host.sql());
    assert!(
        !calls.contains("CREATE DATABASE") && !calls.contains("DROP"),
        "{calls}"
    );
}

#[test]
fn a_foreign_container_under_the_server_name_is_never_adopted() {
    let host = Host::new("foreign");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::write(
        host.base.join("state/c").join(&server),
        "dev.berth.installation=someone-else\n",
    )
    .expect("forge");
    let b = host.checkout("b", PROJECT);
    let up = host.berth(&b, &["up", "--no-wait"]);
    assert_eq!(up.status.code(), Some(125));
    assert!(
        text(&up.stderr).contains("is not this installation's shared postgres server"),
        "{}",
        text(&up.stderr)
    );
}

const TWO_DATABASES: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n  analytics:\n    preset: postgres\n";

fn attachment(host: &Host, network_prefix: &str) -> Vec<String> {
    let server = host.servers().pop().expect("server");
    let dir = host.base.join("state/att").join(server);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut aliases: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(network_prefix)
        })
        .flat_map(|entry| {
            std::fs::read_to_string(entry.path())
                .expect("aliases")
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    aliases.sort();
    aliases
}

#[test]
fn two_shared_services_answer_under_both_names_and_survive_each_other() {
    let host = Host::new("two");
    let a = host.checkout("a", TWO_DATABASES);
    let up = host.berth(&a, &["up", "--no-wait"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert_eq!(host.servers().len(), 1, "one image, one server");
    assert_eq!(attachment(&host, "berth_shop_a-"), ["analytics", "db"]);
    let stop = host.berth(&a, &["stop", "--service", "analytics"]);
    assert!(stop.status.success(), "{}", text(&stop.stderr));
    assert_eq!(
        attachment(&host, "berth_shop_a-"),
        ["db"],
        "db still resolves"
    );
    let ps = host.berth(&a, &["ps", "--format", "json"]);
    let rows: serde_json::Value = serde_json::from_slice(&ps.stdout).expect("ps JSON");
    let status = |name: &str| {
        rows.as_array()
            .expect("rows")
            .iter()
            .find(|row| row["name"] == name)
            .map(|row| row["status"].clone())
    };
    assert_eq!(status("db"), Some(serde_json::json!("running")), "{rows}");
    assert_eq!(
        status("analytics"),
        Some(serde_json::json!("exited")),
        "{rows}"
    );
    host.ok(&a, &["down"]);
    assert!(attachment(&host, "berth_shop_a-").is_empty());
}

#[test]
fn kill_pause_and_attach_never_reach_the_server() {
    let host = Host::new("kill");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    for verb in ["pause", "unpause"] {
        let output = host.berth(&a, &[verb, "--service", "db"]);
        assert_eq!(output.status.code(), Some(125), "{verb}");
        assert!(
            text(&output.stderr).contains("isolation: dedicated"),
            "{verb}: {}",
            text(&output.stderr)
        );
    }
    let kill = host.berth(&a, &["kill", "--service", "db"]);
    assert!(kill.status.success(), "{}", text(&kill.stderr));
    for line in host.calls().lines() {
        let verb = line.split(' ').next().unwrap_or_default();
        assert!(
            !(matches!(verb, "kill" | "pause" | "unpause" | "attach") && line.contains(&server)),
            "{line}"
        );
    }
    assert!(
        attachment(&host, "berth_shop_a-").is_empty(),
        "kill detaches the tenant"
    );
}

#[test]
fn a_whole_purge_detaches_the_server_from_every_environment_network() {
    let host = Host::new("env-purge");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let staging = host.berth(&a, &["up", "--no-wait", "--env", "staging"]);
    assert!(staging.status.success(), "{}", text(&staging.stderr));
    let purge = host.berth(&a, &["down", "--purge"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert!(attachment(&host, "berth_shop_a-").is_empty());
    assert!(
        host.calls().lines().any(|line| line
            .starts_with("network disconnect --force berth_shop_a-")
            && line.contains("-staging ")),
        "{}",
        host.calls()
    );
    assert!(
        !host.base.join("home/instances/shop").exists()
            || std::fs::read_dir(host.base.join("home/instances/shop"))
                .expect("dir")
                .next()
                .is_none()
    );
}

#[test]
fn a_purge_drops_databases_the_tenant_made_outside_its_namespace() {
    let host = Host::new("owned");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let tenant = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    host.clear();
    let output = host.berth_with_owned(
        &a,
        &["down", "--purge"],
        &tenant,
        &format!("scratch:{tenant} theirs:someone"),
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let sql = host.sql();
    assert!(
        sql.contains("DROP DATABASE IF EXISTS \"scratch\" WITH (FORCE)"),
        "{sql}"
    );
    assert!(!sql.contains("\"theirs\""), "{sql}");
}

#[test]
fn publish_flags_never_give_a_tenant_a_host_endpoint() {
    let host = Host::new("publish-all");
    let a = host.checkout("a", PROJECT);
    for flag in ["-P", "--no-publish-all"] {
        let up = host.berth(&a, &["up", flag, "--no-wait", "--json"]);
        assert!(up.status.success(), "{flag}: {}", text(&up.stderr));
        let document: serde_json::Value = serde_json::from_slice(&up.stdout).expect("up JSON");
        let db = document["services"]
            .as_array()
            .expect("services")
            .iter()
            .find(|svc| svc["name"] == "db")
            .expect("db")
            .clone();
        assert_eq!(db["published"], false, "{flag}: {db}");
        assert!(db["url"].is_null(), "{flag}: {db}");
        assert!(
            document["host_environment"].get("DB_HOST").is_none(),
            "{flag}"
        );
    }
}

#[test]
fn commands_before_the_server_exists_do_not_fail_on_it() {
    let host = Host::new("no-server");
    let a = host.checkout("a", PROJECT);
    for args in [
        &["down"][..],
        &["stop"][..],
        &["kill"][..],
        &["down", "--purge"][..],
    ] {
        let output = host.berth(&a, args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
    let test = host.berth(&a, &["artisan", "--no-tty", "--non-interactive", "test"]);
    assert!(
        !text(&test.stderr).contains("failed to inspect shared server"),
        "{}",
        text(&test.stderr)
    );
    assert!(test.status.success(), "{}", text(&test.stderr));
}

#[test]
fn pausing_the_whole_instance_skips_the_shared_server() {
    let host = Host::new("pause-all");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    for verb in ["pause", "unpause"] {
        let output = host.berth(&a, &[verb]);
        assert!(output.status.success(), "{verb}: {}", text(&output.stderr));
    }
    assert!(
        !host.calls().lines().any(|line| (line.starts_with("pause")
            || line.starts_with("unpause"))
            && line.contains(&server)),
        "{}",
        host.calls()
    );
}

fn mysql_project(image: &str) -> String {
    format!(
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: mysql\n    image: {image}\n  app:\n    preset: laravel\n"
    )
}

fn servers_named(host: &Host, prefix: &str) -> usize {
    std::fs::read_dir(host.base.join("state/c"))
        .expect("containers")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .count()
}

#[test]
fn mysql_family_instances_get_a_schema_and_a_short_user_on_one_server_per_flavor() {
    let host = Host::new("mysql");
    let a = host.checkout("a", &mysql_project("mysql:8.4"));
    let b = host.checkout("b", &mysql_project("mysql:8.4"));
    let m = host.checkout("m", &mysql_project("mariadb:11"));
    for checkout in [&a, &b, &m] {
        let up = host.berth(checkout, &["up", "--no-wait"]);
        assert!(up.status.success(), "{}", text(&up.stderr));
    }
    assert_eq!(servers_named(&host, "berth_shared_mysql_"), 1);
    assert_eq!(servers_named(&host, "berth_shared_mariadb_"), 1);
    let env = env_json(&host, &a)["environment"].clone();
    let user = env["DB_USERNAME"].as_str().expect("user");
    let database = env["DB_DATABASE"].as_str().expect("database");
    assert_eq!(user.len(), 25, "fits MySQL's 32-byte user names: {user}");
    assert!(
        database.starts_with("shop__a-") && database.ends_with("__db"),
        "{database}"
    );
    assert_eq!(env["DB_HOST"], "db");
    let pattern = database.replace('_', "\\_");
    let sql = host.sql();
    assert!(
        sql.contains(&format!(
            "GRANT ALL PRIVILEGES ON `{pattern}`.* TO '{user}'@'%'"
        )),
        "{sql}"
    );
    assert!(
        sql.contains(&format!(
            "GRANT ALL PRIVILEGES ON `{pattern}\\_test\\_%`.* TO '{user}'@'%'"
        )),
        "{sql}"
    );
    let calls = host.calls();
    assert!(
        calls
            .lines()
            .any(|line| line.contains("MYSQL_PWD") && line.contains(" mariadb --protocol=tcp")),
        "{calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| line.contains(" mysql --protocol=tcp")),
        "{calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| line.starts_with("run ") && line.contains("-e MYSQL_ROOT_PASSWORD ")),
        "{calls}"
    );
    assert!(
        calls.lines().any(|line| line.starts_with("run ")
            && line.ends_with("mysql:8.4 --skip-log-bin --max-connections=1000")),
        "MySQL 8's binary log would deny tenants triggers and routines: {calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| line.starts_with("run ")
                && line.ends_with(" mariadb:11 --max-connections=1000")),
        "{calls}"
    );
    for secret in host.secrets() {
        assert!(
            !calls.contains(secret.as_str()),
            "a secret reached a docker argument"
        );
    }
}

#[test]
fn a_mysql_purge_drops_the_instance_schemas_and_users_only() {
    let host = Host::new("mysql-purge");
    let a = host.checkout("a", &mysql_project("mysql:8.4"));
    let b = host.checkout("b", &mysql_project("mysql:8.4"));
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&b, &["up", "--no-wait"]);
    let mine = env_json(&host, &a)["environment"].clone();
    let theirs = env_json(&host, &b)["environment"].clone();
    let (my_db, my_user) = (
        mine["DB_DATABASE"].as_str().expect("db"),
        mine["DB_USERNAME"].as_str().expect("user"),
    );
    let (their_db, their_user) = (
        theirs["DB_DATABASE"].as_str().expect("db"),
        theirs["DB_USERNAME"].as_str().expect("user"),
    );
    let names = format!("mysql sys root {my_db} {my_db}_test_1 {my_user} {their_db} {their_user}");
    host.clear();
    let purge = host.berth_with(&a, &["down", "--purge"], &names);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let sql = host.sql();
    assert!(
        sql.contains(&format!("DROP DATABASE IF EXISTS `{my_db}`;")),
        "{sql}"
    );
    assert!(
        sql.contains(&format!("DROP DATABASE IF EXISTS `{my_db}_test_1`;")),
        "{sql}"
    );
    assert!(
        sql.contains(&format!("DROP USER IF EXISTS '{my_user}'@'%';")),
        "{sql}"
    );
    for kept in [their_db, their_user, "mysql", "sys", "root"] {
        assert!(
            !sql.contains(&format!("`{kept}`")) && !sql.contains(&format!("'{kept}'@")),
            "{kept}: {sql}"
        );
    }
}

#[test]
fn an_authored_server_command_reaches_the_shared_server_and_keys_it() {
    let host = Host::new("command");
    let tuned = PROJECT.replace(
        "    preset: postgres\n",
        "    preset: postgres\n    command: [\"postgres\", \"-c\", \"max_connections=300\"]\n",
    );
    let a = host.checkout("a", &tuned);
    let b = host.checkout("b", PROJECT);
    for checkout in [&a, &b] {
        let up = host.berth(checkout, &["up", "--no-wait"]);
        assert!(up.status.success(), "{}", text(&up.stderr));
    }
    assert_eq!(
        host.servers().len(),
        2,
        "different settings, different servers"
    );
    assert!(
        host.calls()
            .lines()
            .any(|line| line.starts_with("run ")
                && line.ends_with(" postgres -c max_connections=300")),
        "{}",
        host.calls()
    );
}

const REDIS_PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  cache:\n    preset: redis\n  app:\n    preset: laravel\n";

#[test]
fn redis_family_tenants_are_acl_users_confined_to_their_prefix() {
    let host = Host::new("redis");
    let a = host.checkout("a", REDIS_PROJECT);
    let b = host.checkout("b", REDIS_PROJECT);
    let v = host.checkout(
        "v",
        &REDIS_PROJECT.replace("preset: redis", "preset: valkey"),
    );
    for checkout in [&a, &b, &v] {
        let up = host.berth(checkout, &["up", "--no-wait"]);
        assert!(up.status.success(), "{}", text(&up.stderr));
    }
    assert_eq!(servers_named(&host, "berth_shared_redis_"), 1);
    assert_eq!(servers_named(&host, "berth_shared_valkey_"), 1);
    let env = env_json(&host, &a)["environment"].clone();
    let user = env["REDIS_USERNAME"].as_str().expect("user");
    assert!(
        user.starts_with("shop__a-") && user.ends_with("__cache"),
        "{user}"
    );
    assert_eq!(env["REDIS_PREFIX"], format!("{user}:"));
    assert_eq!(env["HORIZON_PREFIX"], format!("{user}:horizon:"));
    assert_eq!(env["REDIS_HOST"], "cache");
    let password = env["REDIS_PASSWORD"].as_str().expect("password");
    let sql = host.sql();
    assert!(
        sql.contains(&format!("ACL SETUSER {user} reset on #")),
        "{sql}"
    );
    assert!(sql.contains(&format!("~{user}:* &{user}:*")), "{sql}");
    assert!(
        !sql.contains(password),
        "only the password's hash reaches the server"
    );
    let seed = std::fs::read_to_string(host.base.join("state/acl-seed")).expect("seeded ACL file");
    assert!(seed.starts_with("user default on #"), "{seed}");
    let calls = host.calls();
    assert!(
        calls
            .lines()
            .any(|line| line.starts_with("run ") && line.contains("--aclfile /data/users.acl")),
        "{calls}"
    );
    for secret in host.secrets() {
        assert!(
            !calls.contains(secret.as_str()) && !seed.contains(secret.as_str()),
            "a secret leaked"
        );
    }
}

#[test]
fn a_redis_purge_deletes_the_instance_users_and_their_keys_only() {
    let host = Host::new("redis-purge");
    let a = host.checkout("a", REDIS_PROJECT);
    let b = host.checkout("b", REDIS_PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&b, &["up", "--no-wait"]);
    let mine = env_json(&host, &a)["environment"]["REDIS_USERNAME"]
        .as_str()
        .expect("user")
        .to_owned();
    let theirs = env_json(&host, &b)["environment"]["REDIS_USERNAME"]
        .as_str()
        .expect("user")
        .to_owned();
    host.clear();
    // Laravel's cache lives in database 1; a key name with a newline must
    // never yield a fragment naming someone else's key.
    std::fs::write(
        host.base.join("state/keys-db1"),
        format!("{mine}:cache\n{mine}:odd\n{theirs}:fragment\n"),
    )
    .expect("database 1 keys");
    let names = format!("default {mine} {theirs} {mine}:k");
    let purge = host.berth_with(&a, &["down", "--purge"], &names);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let sql = host.sql();
    assert!(sql.contains(&format!("ACL DELUSER {mine}\n")), "{sql}");
    assert!(
        sql.contains(&format!("SELECT 0\nUNLINK \"{mine}:k\"")),
        "the tenant's keys are unlinked: {sql}"
    );
    assert!(
        sql.contains(&format!(
            "SELECT 1\nUNLINK \"{mine}:cache\" \"{mine}:odd\"\n"
        )),
        "keys in every logical database are unlinked: {sql}"
    );
    assert!(!sql.contains(&format!("\"{theirs}:fragment\"")), "{sql}");
    assert!(
        !sql.contains(&format!("ACL DELUSER {theirs}")) && !sql.contains("ACL DELUSER default"),
        "{sql}"
    );
    // Every database is scanned in one exec: the server's lock is held
    // meanwhile, and one exec per database kept other instances waiting.
    let calls = host.calls();
    assert!(
        !calls.contains("-n 0 --scan") && !calls.contains("-n 1 --scan"),
        "no exec per database: {calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| line.starts_with(&format!("done sh 16 {mine}:"))),
        "all 16 databases in one exec, for the tenant's prefix: {calls}"
    );
}

#[test]
fn each_test_lease_gets_a_tenant_of_its_own() {
    let host = Host::new("lease");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let tenant = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    host.clear();
    let test = host.berth(&a, &["artisan", "--no-tty", "--non-interactive", "test"]);
    assert!(test.status.success(), "{}", text(&test.stderr));
    let calls = host.calls();
    let run = calls
        .lines()
        .find(|line| line.starts_with("run -d") && line.contains("_app-testing-pool-"))
        .expect("the lease's app container");
    let lease_db = run
        .split(' ')
        .find_map(|word| word.strip_prefix("DB_DATABASE="))
        .expect("the lease's database")
        .to_owned();
    assert!(
        lease_db.starts_with(&format!("{tenant}__")),
        "{lease_db} is a tenant of the instance"
    );
    assert!(
        host.sql().contains(&format!("CREATE ROLE \"{lease_db}\"")),
        "{}",
        host.sql()
    );
}

#[test]
fn a_killed_test_run_frees_its_slot_for_the_next_run_at_once() {
    use std::os::unix::process::CommandExt;
    let host = Host::new("killed");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let args = ["artisan", "--no-tty", "--non-interactive", "test"];
    let mut first = host.command(&a, &args, "", "");
    first
        .env("BERTH_TEST_RUNTIME_POOL_SIZE", "1")
        .env("FAKE_TEST_SLEEP", "60")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let mut child = first.spawn().expect("first run");
    let started = std::time::Instant::now();
    while !host.calls().contains("'php' 'artisan' 'test'") {
        assert!(
            started.elapsed().as_secs() < 30,
            "the first run never started its tests"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let group = format!("-{}", child.id());
    assert!(
        Command::new("kill")
            .args(["-9", "--", &group])
            .status()
            .expect("kill")
            .success()
    );
    drop(child.wait());
    let second_started = std::time::Instant::now();
    let second = host
        .command(&a, &args, "", "")
        .env("BERTH_TEST_RUNTIME_POOL_SIZE", "1")
        .output()
        .expect("second run");
    assert!(second.status.success(), "{}", text(&second.stderr));
    assert!(
        second_started.elapsed().as_secs() < 30,
        "the killed run's slot was free at once, not after a timeout"
    );
}

fn gc(host: &Host, cwd: &Path, names: &str) -> serde_json::Value {
    let output = host.berth_with(cwd, &["gc", "--json"], names);
    assert!(output.status.success(), "{}", text(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("one gc document")
}

#[test]
fn gc_removes_a_shared_server_no_instance_uses_with_its_volume_and_secret() {
    let host = Host::new("gc");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.ok(&a, &["down", "--purge"]);
    host.clear();
    let document = gc(&host, &host.base, "");
    assert_eq!(document["servers"][0]["server"], server.as_str());
    assert_eq!(document["servers"][0]["removed"], true, "{document}");
    // No instance network holds the gateway any more, so it goes too; its CA
    // stays under the berth home for the next `up`.
    assert_eq!(document["gateway"]["removed"], true, "{document}");
    let calls = host.calls();
    assert!(
        calls
            .lines()
            .any(|line| line == format!("rm -f -v {server}")),
        "{calls}"
    );
    assert!(
        calls
            .lines()
            .any(|line| line == format!("volume rm {server}-data")),
        "{calls}"
    );
    assert!(
        !host.base.join("home/shared").join(&server).exists(),
        "its admin secret is gone"
    );
    assert!(host.servers().is_empty());
}

#[test]
fn gc_keeps_the_admin_secret_when_the_engine_cannot_remove_the_server() {
    let host = Host::new("gc-dial");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.ok(&a, &["down", "--purge"]);
    let output = host
        .command(&host.base, &["gc", "--json"], "", "")
        .env("FAKE_RM_DIAL", "1")
        .output()
        .expect("gc");
    assert!(!output.status.success(), "{}", text(&output.stdout));
    assert!(
        host.base.join("home/shared").join(&server).exists(),
        "a server the engine did not remove keeps its admin secret"
    );
}

#[test]
fn gc_refuses_dry_run_and_removes_nothing() {
    let host = Host::new("gc-dry");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&a, &["down", "--purge"]);
    host.clear();
    let output = host.berth_with(&host.base, &["--dry-run", "gc", "--json"], "");
    assert_eq!(output.status.code(), Some(125));
    assert!(text(&output.stderr).contains("does not support --dry-run"));
    assert_eq!(host.servers().len(), 1, "the server is still there");
    assert!(host.calls().is_empty(), "{}", host.calls());
}

#[test]
fn gc_keeps_servers_an_instance_uses_or_that_hold_tenants() {
    let host = Host::new("gc-keep");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&b, &["up", "--no-wait"]);
    let attached = gc(&host, &host.base, "");
    assert_eq!(attached["servers"][0]["removed"], false);

    assert!(
        attached["servers"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("attached to")),
        "{attached}"
    );

    let tenant = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    host.ok(&a, &["down"]);
    host.ok(&b, &["down"]);
    let holding = gc(&host, &host.base, &tenant);
    assert_eq!(
        holding["servers"][0]["removed"], false,
        "`down` keeps data: {holding}"
    );
    assert!(
        holding["servers"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains(&tenant)),
        "{holding}"
    );
    assert_eq!(host.servers().len(), 1);
}

#[test]
fn gc_keeps_a_stopped_server_it_cannot_inspect_and_never_starts_it() {
    let host = Host::new("gc-stopped");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.ok(&a, &["down", "--purge"]);
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    let document = gc(&host, &host.base, "");
    assert_eq!(document["servers"][0]["removed"], false, "{document}");
    assert!(
        document["servers"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("exited")),
        "{document}"
    );
    let calls = host.calls();
    assert!(
        !calls.lines().any(
            |line| (line.starts_with("start ") || line.starts_with("rm "))
                && line.contains(&server)
        ),
        "{calls}"
    );
}

#[test]
fn a_tenant_that_fails_to_provision_names_its_instance_and_leaves_others_up() {
    let host = Host::new("tenant-fails");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let instance_b = env_json(&host, &b)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant of b")
        .to_owned();
    std::fs::write(host.base.join("state").join("tenant-fails"), b"").expect("break tenants");

    let failed = host.berth(&b, &["up", "--no-wait"]);
    assert_eq!(failed.status.code(), Some(125));
    let stderr = text(&failed.stderr);
    assert!(
        stderr.contains(&format!(
            "failed to provision tenant {instance_b} of instance shop/b-"
        )),
        "{stderr}"
    );
    assert!(
        stderr.contains("permission denied to create role"),
        "{stderr}"
    );
    assert_eq!(host.servers().len(), 1, "the shared server stays up for a");
}

#[test]
fn a_shared_server_that_stops_or_rejects_berth_fails_up_at_once_without_a_cap() {
    for (mode, message) in [
        ("exits", "stopped while starting"),
        ("badauth", "rejects berth's admin password"),
        ("norole", "role \"berth_admin\" does not exist"),
    ] {
        let host = Host::new(&format!("server-{mode}"));
        let a = host.checkout("a", PROJECT);
        let started = std::time::Instant::now();
        let up = bounded_output(
            host.command(&a, &["up", "--no-wait"], "", "")
                .env("FAKE_SERVER", mode),
        );
        assert_eq!(up.status.code(), Some(125), "{mode}: {}", text(&up.stderr));
        assert!(started.elapsed().as_secs() < 20, "{mode}: no endless wait");
        assert!(
            text(&up.stderr).contains(message),
            "{mode}: {}",
            text(&up.stderr)
        );
    }
}

#[test]
fn reaping_in_passing_skips_a_busy_shared_server_instead_of_waiting_for_it() {
    let host = Host::new("reap-busy");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    host.ok(&a, &["up", "--no-wait", "--ttl", "1s"]);
    host.ok(&b, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    drop(std::fs::remove_file(
        host.base.join("home/locks/reap.stamp"),
    ));
    // Another berth process is using the server for as long as this test
    // holds its lock.
    let busy = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(host.base.join(format!("home/locks/shared-{server}.lock")))
        .expect("lock file");
    busy.lock().expect("hold the server's lock");

    let started = std::time::Instant::now();
    let ps = host.berth(&b, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "another instance's leftovers never block a command"
    );
    assert!(
        text(&ps.stderr).contains("another berth process is using it"),
        "{}",
        text(&ps.stderr)
    );
    drop(busy);
}

#[test]
fn reaping_in_passing_skips_a_shared_server_that_refuses_connections() {
    let host = Host::new("reap-refusing");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    host.ok(&a, &["up", "--no-wait", "--ttl", "1s"]);
    host.ok(&b, &["up", "--no-wait"]);
    std::thread::sleep(std::time::Duration::from_millis(2100));
    drop(std::fs::remove_file(
        host.base.join("home/locks/reap.stamp"),
    ));
    // From now on the running server refuses every connection.
    std::fs::write(host.base.join("state/refuse"), "").expect("refuse");
    let started = std::time::Instant::now();
    let ps = bounded_output(
        host.command(&b, &["ps"], "", "")
            .env("FAKE_SERVER", "refusing"),
    );
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "another instance's leftovers never block a command"
    );
    assert!(
        text(&ps.stderr).contains("skipped shared server"),
        "{}",
        text(&ps.stderr)
    );
}

#[test]
fn a_reset_on_a_server_where_the_checks_cannot_hold_drops_nothing() {
    // PostgreSQL before 10.7 (and 11.0, 11.1) lets a dump hide its errors:
    // berth's start query returns no row there, and the check before the
    // reset refuses it.
    let host = Host::new("reset-old-server");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    std::fs::write(
        a.join("dump.sql"),
        "CREATE TABLE restored_marker (id int);\n",
    )
    .expect("dump");
    host.clear();
    let restore = bounded_output(
        host.command(
            &a,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_OLD_SERVER", "9"),
    );
    assert!(!restore.status.success());
    // A 9.x server's end query errors too: the version is still named.
    assert!(
        text(&restore.stderr).contains("older than 10.7"),
        "{}",
        text(&restore.stderr)
    );
    assert!(!host.sql().contains("DROP"), "{}", host.sql());
    // Without `--reset` too: the dump never runs.
    host.clear();
    let plain = bounded_output(
        host.command(
            &a,
            &["restore", "--service", "db", "--file", "dump.sql"],
            "",
            "",
        )
        .env("FAKE_OLD_SERVER", "1"),
    );
    assert!(!plain.status.success());
    assert!(
        text(&plain.stderr).contains("older than 10.7"),
        "{}",
        text(&plain.stderr)
    );
    // The tenant's client is a throwaway container: its stdin is logged.
    let stdin = std::fs::read_to_string(host.base.join("state/client-stdin")).unwrap_or_default();
    assert!(!stdin.contains("restored_marker"), "{stdin}");
}

#[test]
fn a_reset_restore_reprovisions_only_the_instance_tenant() {
    let host = Host::new("reset");
    let a = host.checkout("a", PROJECT);
    let b = host.checkout("b", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&b, &["up", "--no-wait"]);
    let tenant_a = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant a")
        .to_owned();
    let tenant_b = env_json(&host, &b)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant b")
        .to_owned();
    std::fs::write(
        a.join("dump.sql"),
        "CREATE TABLE restored_marker (id int);\n",
    )
    .expect("dump");
    host.clear();

    // The fake lists the tenants the server holds from these names.
    let restore = host.berth_with(
        &a,
        &[
            "restore",
            "--service",
            "db",
            "--reset",
            "--file",
            "dump.sql",
        ],
        &format!("{tenant_a} {tenant_b}"),
    );
    assert!(restore.status.success(), "{}", text(&restore.stderr));
    let sql = host.sql();
    let dropped = sql.find(&format!(
        "DROP DATABASE IF EXISTS \"{tenant_a}\" WITH (FORCE)"
    ));
    assert!(dropped.is_some(), "tenant a dropped: {sql}");
    let dropped = dropped.expect("tenant a dropped");
    let created = sql[dropped..].find(&format!("'CREATE DATABASE ' || quote_ident('{tenant_a}')"));
    assert!(created.is_some(), "tenant a provisioned again: {sql}");
    assert!(
        !sql.contains("restored_marker"),
        "the dump never reaches the shared server's own client: {sql}"
    );
    let client_stdin =
        std::fs::read_to_string(host.base.join("state/client-stdin")).expect("client stdin");
    assert!(
        client_stdin.contains("restored_marker"),
        "the dump reaches the tenant's throwaway client: {client_stdin}"
    );
    assert!(
        !sql.contains(&tenant_b),
        "instance b's tenant is untouched: {sql}"
    );
}

#[test]
fn restore_and_dump_of_a_shared_tenant_run_in_a_throwaway_client_for_every_sql_engine() {
    for (label, project, client, dump) in [
        ("restore-pg", PROJECT.to_owned(), "psql", "pg_dump"),
        (
            "restore-mysql",
            mysql_project("mysql:8.4"),
            "mysql",
            "mysqldump",
        ),
        (
            "restore-maria",
            mysql_project("mariadb:11.4"),
            "mariadb",
            "mariadb-dump",
        ),
    ] {
        let host = Host::new(label);
        let a = host.checkout("a", &project);
        host.ok(&a, &["up", "--no-wait"]);
        let server = std::fs::read_dir(host.base.join("state/c"))
            .expect("containers")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with("berth_shared_"))
            .expect("server");
        let password = env_json(&host, &a)["environment"]["DB_PASSWORD"]
            .as_str()
            .expect("password")
            .to_owned();
        std::fs::write(
            a.join("dump.sql"),
            "CREATE TABLE restored_marker (id int);\n",
        )
        .expect("dump");
        host.clear();

        let restore = host.berth(&a, &["restore", "--service", "db", "--file", "dump.sql"]);
        assert!(
            restore.status.success(),
            "{label}: {}",
            text(&restore.stderr)
        );
        let calls = host.calls();
        let run = calls
            .lines()
            .find(|line| line.starts_with("run --rm"))
            .unwrap_or_default()
            .to_owned();
        assert!(
            run.contains(&format!("--entrypoint {client} ")) && run.contains(" -h db "),
            "{label}: the dump goes to the tenant's own client: {run}"
        );
        // Its errors are judged by their English wording.
        assert_eq!(
            run.contains("-e LC_ALL=C "),
            client == "psql",
            "{label}: {run}"
        );
        // Each error comes with the statement that failed, and the session
        // starts with standard strings whatever the tenant set for its role.
        if client == "psql" {
            assert!(
                run.contains(" -X -b -v ON_ERROR_ROLLBACK=on -c ")
                    && run.contains(" -f - -c ")
                    && run.contains("berth-backend-end"),
                "{run}"
            );
            assert!(
                run.contains("-e PGOPTIONS=-c standard_conforming_strings=on "),
                "{run}"
            );
        }

        assert!(
            !calls.lines().any(|line| line.starts_with("exec")
                && line.contains(&server)
                && line.contains(client)),
            "{label}: no client runs inside the shared server: {calls}"
        );
        assert!(
            std::fs::read_to_string(host.base.join("state/client-stdin"))
                .unwrap_or_default()
                .contains("restored_marker"),
            "{label}: the dump reaches the throwaway client"
        );
        assert!(!calls.contains(&password), "{label}: no password in argv");

        host.clear();
        let dumped = host.berth(&a, &["dump", "--service", "db", "--stdout"]);
        assert!(dumped.status.success(), "{label}: {}", text(&dumped.stderr));
        let calls = host.calls();
        let run = calls
            .lines()
            .find(|line| line.starts_with("run --rm"))
            .unwrap_or_default()
            .to_owned();
        assert!(
            run.contains(&format!("--entrypoint {dump} ")) && run.contains(" -h db "),
            "{label}: the dump program runs in a throwaway container: {run}"
        );
        if client == "psql" {
            // A dump naming its roles fails to restore into another tenant.
            assert!(run.contains("--no-owner --no-privileges"), "{label}: {run}");
        } else {
            assert!(run.contains("--no-tablespaces"), "{label}: {run}");
        }
        assert!(
            !calls.lines().any(|line| line.starts_with("exec")
                && line.contains(&server)
                && line.contains(dump)),
            "{label}: no dump runs inside the shared server: {calls}"
        );
        assert!(!calls.contains(&password), "{label}: no password in argv");
    }
}

#[test]
fn commands_of_the_callers_choosing_never_run_in_a_shared_server() {
    let host = Host::new("exec-shared");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.clear();
    let attempts: [&[&str]; 4] = [
        &[
            "exec",
            "--no-tty",
            "--service",
            "db",
            "--",
            "sh",
            "-c",
            "id",
        ],
        &["cp", "db:/var/lib/postgresql", "./copy"],
        &["cp", "./x", "db:/tmp/x"],
        &["inspect", "--service", "db"],
    ];
    for args in attempts {
        let output = host.berth(&a, args);
        assert_eq!(output.status.code(), Some(125), "{args:?}");
        assert!(
            text(&output.stderr).contains("isolation: dedicated"),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
    assert!(
        !host.calls().lines().any(|line| line.contains(&server)
            && (line.starts_with("exec ")
                || line.starts_with("cp ")
                || line.starts_with("inspect "))
            && !line.contains("--format")),
        "{}",
        host.calls()
    );
    let raw = host.berth(
        &a,
        &["cp", &format!("{server}:/var/lib/postgresql"), "./copy"],
    );
    assert_eq!(raw.status.code(), Some(125), "{}", text(&raw.stderr));
}

#[test]
fn the_tenant_client_runs_in_a_throwaway_container_on_the_instance_network() {
    let host = Host::new("client");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let env = env_json(&host, &a);
    let tenant = env["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let password = env["environment"]["DB_PASSWORD"]
        .as_str()
        .expect("password")
        .to_owned();
    let network = format!(
        "berth_shop_{}",
        tenant.trim_start_matches("shop__").trim_end_matches("__db")
    );
    host.clear();
    let client = host.berth(&a, &["exec", "--no-tty", "--service", "db"]);
    assert!(client.status.success(), "{}", text(&client.stderr));
    let calls = host.calls();
    assert!(
        !calls
            .lines()
            .any(|line| line.starts_with("exec") && line.contains(&server)),
        "the client never runs inside the shared server: {calls}"
    );
    let runs: Vec<&str> = calls
        .lines()
        .filter(|line| line.starts_with("run "))
        .collect();
    assert_eq!(runs.len(), 1, "{calls}");
    let run = runs[0];
    let words: Vec<&str> = run.split(' ').collect();
    for expected in ["--rm", "-i", "--entrypoint", "psql", "-h", "db", "-U"] {
        assert!(words.contains(&expected), "{expected}: {run}");
    }
    assert!(!words.contains(&"-it"), "{run}");
    assert!(run.contains(&format!("--network {network}")), "{run}");
    assert!(run.contains("timescale/timescaledb-ha:pg18"), "{run}");
    assert!(words.contains(&tenant.as_str()), "{run}");
    for forbidden in [
        "-v",
        "--volume",
        "--mount",
        "--privileged",
        "host",
        "docker.sock",
    ] {
        assert!(!words.contains(&forbidden), "{forbidden}: {run}");
    }
    assert!(
        !calls.contains(&password),
        "the password never reaches argv: {calls}"
    );
    let client_env =
        std::fs::read_to_string(host.base.join("state/client-env")).unwrap_or_default();
    assert_eq!(client_env.trim(), "PGPASSWORD", "{client_env}");
}

#[test]
fn the_tenant_clients_exit_code_and_stdin_pass_through() {
    use std::io::Write as _;
    let host = Host::new("client-exit");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let mut child = host
        .command(&a, &["exec", "--no-tty", "--service", "db"], "", "")
        .env("FAKE_CLIENT_EXIT", "3")
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn berth");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"select 1;\n")
        .expect("write");
    let output = child.wait_with_output().expect("berth");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output.stderr));
    assert_eq!(
        std::fs::read_to_string(host.base.join("state/client-stdin")).expect("stdin"),
        "select 1;\n"
    );
}

#[test]
fn the_tenant_client_needs_the_instance_attached() {
    let host = Host::new("client-detached");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&a, &["stop", "--service", "db"]);
    host.clear();
    let client = host.berth(&a, &["exec", "--no-tty", "--service", "db"]);
    assert_eq!(client.status.code(), Some(125), "{}", text(&client.stderr));
    assert!(
        text(&client.stderr).contains(" up` first"),
        "{}",
        text(&client.stderr)
    );
    assert!(
        !host.calls().lines().any(|line| line.starts_with("run ")),
        "{}",
        host.calls()
    );
}

#[test]
fn the_tenant_client_needs_a_running_server() {
    let host = Host::new("client-stopped");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    let client = host.berth(&a, &["exec", "--no-tty", "--service", "db"]);
    assert_eq!(client.status.code(), Some(125), "{}", text(&client.stderr));
    assert!(
        text(&client.stderr).contains(" up` first"),
        "{}",
        text(&client.stderr)
    );
    assert!(
        !host.calls().lines().any(|line| line.starts_with("run ")),
        "{}",
        host.calls()
    );
}

#[test]
fn on_podman_the_server_and_gateway_start_on_a_network_instances_can_join() {
    // Rootless Podman gives a container started without `--network` its
    // own slirp4netns or pasta stack, and `network connect` then fails
    // ("slirp4netns" is not supported: invalid network mode).
    let host = Host::new("podman-net");
    let a = host.checkout("a", PROJECT);
    let up = host
        .command(&a, &["up", "--no-wait"], "", "")
        .env("BERTH_ENGINE", "podman")
        .output()
        .expect("run berth");
    assert!(up.status.success(), "{}", text(&up.stderr));
    let calls = host.calls();
    for (what, marker) in [
        ("shared server", "berth_shared_postgres_"),
        ("gateway", "caddy run"),
    ] {
        let run = calls
            .lines()
            .find(|line| line.starts_with("run ") && line.contains(marker));
        assert!(
            run.is_some_and(|run| run.contains("--network podman")),
            "{what}: {calls}"
        );
    }
    // Podman 4.9's listings have no `.Label` function.
    for line in calls.lines().filter(|line| line.contains("--format")) {
        assert!(!line.contains(".Label \""), "{line}");
    }
    let listing = host
        .command(&a, &["ls", "--json"], "", "")
        .env("BERTH_ENGINE", "podman")
        .output()
        .expect("run berth");
    assert!(listing.status.success(), "{}", text(&listing.stderr));
    for line in host
        .calls()
        .lines()
        .filter(|line| line.contains("--format"))
    {
        assert!(!line.contains(".Label \""), "{line}");
    }
    let docker = Host::new("docker-net");
    let b = docker.checkout("b", PROJECT);
    docker.ok(&b, &["up", "--no-wait"]);
    assert!(
        !docker.calls().contains("--network podman"),
        "{}",
        docker.calls()
    );
}

#[test]
fn on_podman_a_server_created_without_its_network_is_recreated_keeping_its_volume() {
    // A server an earlier berth started with no `--network` keeps rootless
    // Podman's private stack and can never be attached to an instance.
    let host = Host::new("podman-heal");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    // On its private stack it reports no networks: every attach failed.
    std::fs::remove_dir_all(host.base.join("state/att").join(&server)).expect("detach");
    host.clear();
    let up = host
        .command(&a, &["up", "--no-wait"], "", "")
        .env("BERTH_ENGINE", "podman")
        .output()
        .expect("run berth");
    assert!(up.status.success(), "{}", text(&up.stderr));
    let calls = host.calls();
    let lines: Vec<&str> = calls.lines().collect();
    let removed = lines
        .iter()
        .position(|line| line.starts_with("rm -f") && line.split(' ').any(|word| word == server));
    let recreated = lines.iter().position(|line| {
        line.starts_with("run -d") && line.contains(&server) && line.contains("--network podman")
    });
    assert!(
        removed.is_some() && recreated.is_some() && removed < recreated,
        "{calls}"
    );
    assert!(
        !calls.lines().any(|line| line.starts_with("volume rm")),
        "the server's data survives: {calls}"
    );
    assert!(
        !calls
            .lines()
            .any(|line| line.starts_with("volume create") && line.contains(&server)),
        "an existing volume is reused, not created again: {calls}"
    );
}

#[test]
fn on_podman_a_server_on_any_network_is_kept() {
    // A server on a bridge (even one not named `podman`) attaches fine;
    // recreating it would detach every other instance.
    let host = Host::new("podman-keep");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.clear();
    let up = host
        .command(&a, &["up", "--no-wait"], "", "")
        .env("BERTH_ENGINE", "podman")
        .output()
        .expect("run berth");
    assert!(up.status.success(), "{}", text(&up.stderr));
    let calls = host.calls();
    assert!(
        !calls
            .lines()
            .any(|line| line.starts_with("rm -f") && line.split(' ').any(|word| word == server)),
        "{calls}"
    );
}

#[test]
fn on_podman_a_server_is_never_recreated_without_its_run_options() {
    // A target read back from labels carries no run options (`--aclfile`,
    // connection limits); recreating from it would start the server
    // without them.
    let host = Host::new("podman-labels");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::remove_dir_all(host.base.join("state/att").join(&server)).expect("detach");
    host.clear();
    for args in [&["gc"][..], &["down", "--purge"][..]] {
        let output = host
            .command(&a, args, "", "")
            .env("BERTH_ENGINE", "podman")
            .output()
            .expect("run berth");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
    // `down --purge` in the checkout knows the project's options and may
    // repair the server; nothing may start it without them.
    let calls = host.calls();
    for run in calls
        .lines()
        .filter(|line| line.starts_with("run -d") && line.contains(&server))
    {
        assert!(run.ends_with("-c max_connections=1000"), "{run}");
    }
}

/// Runs `args` while this test holds `a`'s in-use lock as the reaper does,
/// with its credentials unreadable; then purges the instance as a reap
/// would and releases the lock. Returns the command's output.
fn run_through_a_reap(
    host: &Host,
    a: &Path,
    instance: &str,
    selector: &[&str],
    command: &[&str],
) -> Output {
    let args = [selector, command].concat();
    let env = host.ok(a, &[selector, &["env", "--json"]].concat());
    let document: serde_json::Value = serde_json::from_slice(&env.stdout).expect("env JSON");
    let tenant = document["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let state = host.base.join("home/instances/shop").join(instance);
    let credentials = state.join("credentials").join(&tenant);
    let lock_path = host
        .base
        .join("home/locks")
        .join(format!("instance_shop_{instance}.lock"));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .expect("lock file");
    lock.lock().expect("reaper lock");
    std::fs::write(&credentials, "not hex").expect("unreadable credentials");
    let mut child = host
        .command(a, &args, "", "")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn berth");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    assert!(
        child.try_wait().expect("poll").is_none(),
        "{args:?} read the instance's state before the reap released it"
    );
    std::fs::remove_dir_all(&state).expect("purge state");
    std::fs::remove_file(&lock_path).expect("purge lock");
    drop(lock);
    child.wait_with_output().expect("berth output")
}

#[test]
fn a_command_waits_for_a_reap_in_progress_before_reading_its_credentials() {
    let host = Host::new("reap-race");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let validated: serde_json::Value =
        serde_json::from_slice(&host.ok(&a, &["config", "validate", "--json"]).stdout)
            .expect("validate");
    let instance = validated["instance"].as_str().expect("instance").to_owned();
    let ps = run_through_a_reap(&host, &a, &instance, &[], &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(
        !text(&ps.stderr).contains("valid secret"),
        "{}",
        text(&ps.stderr)
    );

    host.ok(&a, &["--instance", "x", "up", "--no-wait"]);
    let named = run_through_a_reap(&host, &a, "x", &["--instance", "x"], &["ps"]);
    assert!(named.status.success(), "{}", text(&named.stderr));

    // Commands that never touch Docker read and create credentials too.
    let env = run_through_a_reap(&host, &a, &instance, &[], &["env", "--json"]);
    assert!(env.status.success(), "{}", text(&env.stderr));
    let document: serde_json::Value = serde_json::from_slice(&env.stdout).expect("env JSON");
    let password = document["environment"]["DB_PASSWORD"]
        .as_str()
        .expect("password");
    assert_eq!(
        env_json(&host, &a)["environment"]["DB_PASSWORD"],
        password,
        "env printed the credentials berth keeps"
    );
}

#[test]
fn up_json_credentials_match_what_berth_keeps_after_waiting_out_a_reap() {
    let host = Host::new("reap-up");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let validated: serde_json::Value =
        serde_json::from_slice(&host.ok(&a, &["config", "validate", "--json"]).stdout)
            .expect("validate");
    let instance = validated["instance"].as_str().expect("instance").to_owned();
    let up = run_through_a_reap(&host, &a, &instance, &[], &["up", "--no-wait", "--json"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document: serde_json::Value = serde_json::from_slice(&up.stdout).expect("up JSON");
    let reported = document["credentials"]["db"]["password"]
        .as_str()
        .expect("password")
        .to_owned();
    let env = env_json(&host, &a);
    assert_eq!(
        env["environment"]["DB_PASSWORD"],
        reported.as_str(),
        "{document}"
    );
    assert!(
        reported.len() == 64 && reported.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "a fresh credential: {reported}"
    );
}

#[test]
fn each_engines_tenant_client_reaches_its_server_by_name_with_its_password_in_the_environment() {
    for (label, project, service, program, password_env, port) in [
        (
            "client-mysql",
            "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: mysql\n",
            "db",
            "mysql",
            "MYSQL_PWD",
            "3306",
        ),
        (
            "client-mariadb",
            "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: mariadb\n",
            "db",
            "mariadb",
            "MYSQL_PWD",
            "3306",
        ),
        (
            "client-redis",
            REDIS_PROJECT,
            "cache",
            "redis-cli",
            "REDISCLI_AUTH",
            "6379",
        ),
    ] {
        let host = Host::new(label);
        let a = host.checkout("a", project);
        host.ok(&a, &["up", "--no-wait"]);
        host.clear();
        let client = host
            .command(&a, &["exec", "--service", service], "", "")
            .output()
            .expect("run berth");
        assert!(client.status.success(), "{label}: {}", text(&client.stderr));
        let calls = host.calls();
        let run = calls
            .lines()
            .find(|line| line.starts_with("run --rm"))
            .unwrap_or_default()
            .to_owned();
        let words: Vec<&str> = run.split(' ').collect();
        assert!(
            run.contains(&format!("--entrypoint {program} ")),
            "{label}: {run}"
        );
        assert!(run.contains(&format!("-h {service} ")), "{label}: {run}");
        assert!(
            words.contains(&port) || words.contains(&format!("--port={port}").as_str()),
            "{label}: {run}"
        );
        for hardening in ["--cap-drop ALL", "--security-opt no-new-privileges"] {
            assert!(run.contains(hardening), "{label}: {run}");
        }
        let env = env_json(&host, &a);
        let user_key = if service == "cache" {
            "REDIS_USERNAME"
        } else {
            "DB_USERNAME"
        };
        let user = env["environment"][user_key]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(!user.is_empty(), "{label}: {env}");
        let user_flag = if service == "cache" { "--user" } else { "-u" };
        assert!(
            run.contains(&format!("{user_flag} {user}")),
            "{label}: {run}"
        );
        if service == "db" {
            let database = env["environment"]["DB_DATABASE"]
                .as_str()
                .expect("database");
            assert!(run.ends_with(&format!(" {database}")), "{label}: {run}");
        }
        assert!(
            words.contains(&"-i") && !words.contains(&"-it"),
            "without a terminal: {run}"
        );
        let client_env =
            std::fs::read_to_string(host.base.join("state/client-env")).unwrap_or_default();
        assert_eq!(client_env.trim(), password_env, "{label}");
    }
}

#[test]
fn gc_removes_a_mariadb_server_whose_only_users_are_the_images_own() {
    // The official image creates `healthcheck` for its healthcheck script;
    // it is nobody's tenant.
    let host = Host::new("gc-maria");
    let a = host.checkout(
        "a",
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: mariadb\n",
    );
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&a, &["down", "--purge"]);
    let document = gc(
        &host,
        &host.base,
        "root mariadb.sys healthcheck mysql information_schema performance_schema sys",
    );
    let server = &document["servers"][0];
    assert_eq!(server["engine"], "mariadb", "{document}");
    assert_eq!(server["removed"], true, "{document}");
}

/// Runs `command` to completion, failing the test instead of hanging when a
/// regression makes berth wait forever (berth itself has no deadlines).
fn bounded_output(command: &mut Command) -> Output {
    use std::io::Read;
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run berth");
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                drop(pipe.read_to_end(&mut bytes));
            }
            bytes
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for berth") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(120) {
            timed_out = true;
            drop(child.kill());
            break child.wait().expect("wait for the killed berth");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(
        !timed_out,
        "berth did not finish within 120s: a wait that should end did not"
    );
    Output {
        status,
        stdout: stdout.join().expect("stdout"),
        stderr: stderr.join().expect("stderr"),
    }
}

#[test]
fn a_corrupt_admin_secret_on_a_running_server_names_its_recovery_and_file() {
    let host = Host::new("corrupt-admin");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    let shared = host.base.join("home/shared");
    let server = std::fs::read_dir(&shared)
        .expect("shared")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.join("admin").is_file())
        .expect("a server's admin secret");
    std::fs::write(server.join("admin"), "not a secret").expect("corrupt");

    let up = host.berth(&checkout, &["up", "--no-wait"]);
    assert!(!up.status.success(), "{}", text(&up.stderr));
    let stderr = text(&up.stderr);
    // The server never saw a password: no connection, logs or rerun advice.
    assert!(
        !stderr.contains("does not accept berth's admin connection"),
        "{stderr}"
    );
    assert!(!stderr.contains("invalid project file"), "{stderr}");
    assert!(stderr.contains("admin secret"), "{stderr}");
    assert!(stderr.contains("openssl rand -hex 32"), "{stderr}");
    assert!(
        stderr.contains(&format!("{}: ", server.join("admin").display())),
        "the advice names the corrupt file first: {stderr}"
    );
}

#[test]
fn a_corrupt_tenant_password_is_a_home_file_error_not_a_project_file_one() {
    let host = Host::new("corrupt-tenant");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    let mut stack = vec![host.base.join("home/instances")];
    let mut password = None;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir").filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .parent()
                .is_some_and(|parent| parent.ends_with("credentials"))
            {
                password = Some(path);
            }
        }
    }
    let password = password.expect("a tenant password");
    std::fs::write(&password, "not a secret").expect("corrupt");

    let output = host.berth(&checkout, &["ps"]);
    assert!(!output.status.success(), "{}", text(&output.stdout));
    let stderr = text(&output.stderr);
    assert!(!stderr.contains("invalid project file"), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "{}: the file is a tenant password",
            password.display()
        )),
        "{stderr}"
    );
}

#[test]
fn a_corrupt_mysql_admin_secret_says_to_delete_the_file_after_removing_the_server() {
    let host = Host::new("corrupt-mysql-admin");
    let project = PROJECT.replace("preset: postgres", "preset: mysql");
    let checkout = host.checkout("a", &project);
    host.ok(&checkout, &["up", "--no-wait"]);
    let admin = std::fs::read_dir(host.base.join("home/shared"))
        .expect("shared")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("admin"))
        .find(|path| path.is_file())
        .expect("a server's admin secret");
    std::fs::write(&admin, "not a secret").expect("corrupt");

    let up = host.berth(&checkout, &["up", "--no-wait"]);
    assert!(!up.status.success(), "{}", text(&up.stderr));
    let stderr = text(&up.stderr);
    // berth never replaces an invalid file, so the loop only ends with it.
    assert!(
        stderr.contains(&format!("delete the file (`rm {}`)", admin.display())),
        "{stderr}"
    );
    assert!(!stderr.contains("openssl rand"), "{stderr}");
}

#[test]
fn a_reset_restore_from_a_bad_dump_drops_nothing() {
    let host = Host::new("reset-bad-file");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    std::fs::write(checkout.join("empty.sql"), "").expect("empty dump");
    std::fs::write(checkout.join("plain.sql.gz"), "select 1;").expect("not gzip");
    std::fs::create_dir_all(checkout.join("backups")).expect("directory");
    // `gzip < /dev/null`: valid gzip that decompresses to nothing.
    let empty_gzip: [u8; 20] = [
        31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    std::fs::write(checkout.join("nothing.sql.gz"), empty_gzip).expect("gzip of nothing");
    // Gzip under a plain name, as `berth dump --gzip --file backup.sql` writes.
    std::fs::write(checkout.join("backup.sql"), empty_gzip).expect("gzip, plain name");
    // A pg_dump without its closing line: the transfer was cut short.
    std::fs::write(
        checkout.join("cut.sql"),
        "--\n-- PostgreSQL database dump\n--\n\nCREATE TABLE users (id int);\n",
    )
    .expect("cut-short dump");
    // Hypertable chunks live in TimescaleDB's own schemas: a tenant of a
    // shared server cannot write them.
    std::fs::write(
        checkout.join("hyper.sql"),
        "--\n-- PostgreSQL database dump\n--\n\nCREATE TABLE \
         _timescaledb_internal._hyper_1_1_chunk (t timestamptz);\n\n--\n-- PostgreSQL \
         database dump complete\n--\n",
    )
    .expect("hypertable dump");
    let file = |name| vec!["restore", "--service", "db", "--reset", "--file", name];
    // stdin is empty here, as when the command feeding it failed.
    let cases = [
        ("empty file", file("empty.sql"), "is empty (0 bytes)"),
        ("missing file", file("missing.sql"), "no SQL dump at"),
        ("directory", file("backups"), "is not a dump file"),
        (
            "plain file read as gzip",
            file("plain.sql.gz"),
            "is not gzip-compressed",
        ),
        (
            "gzip of nothing",
            file("nothing.sql.gz"),
            "empty once decompressed",
        ),
        (
            "gzip read as plain SQL",
            file("backup.sql"),
            "pass `--gzip`",
        ),
        (
            "empty stdin",
            vec!["restore", "--service", "db", "--reset"],
            "is empty, so there is nothing to restore",
        ),
        (
            "cut-short dump",
            file("cut.sql"),
            "is cut short: its dump tool's end-of-dump line is missing",
        ),
        (
            "hypertables into a shared server",
            file("hyper.sql"),
            "set `isolation: dedicated` on the service",
        ),
    ];
    for (case, args, reason) in cases {
        host.clear();
        let restore = host.berth(&checkout, &args);
        assert!(
            text(&restore.stderr).contains(reason),
            "{case}: {}",
            text(&restore.stderr)
        );
        assert!(
            !restore.status.success(),
            "{case}: {}",
            text(&restore.stderr)
        );
        // `--reset` must not drop the data a bad dump was meant to replace.
        assert!(
            !host.sql().contains("DROP DATABASE"),
            "{case}: {}",
            host.sql()
        );
    }
}

#[test]
fn a_dump_on_stdin_without_its_end_line_fails_the_restore() {
    use std::io::Write;
    let host = Host::new("stdin-cut");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    let mut child = host
        .command(&checkout, &["restore", "--service", "db"], "", "")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"--\n-- PostgreSQL database dump\n--\n\nCREATE TABLE users (id int);\n")
        .expect("write");
    let restore = child.wait_with_output().expect("restore");
    let stderr = text(&restore.stderr);
    assert!(
        !restore.status.success(),
        "a cut-short stream never succeeds"
    );
    // Kept and checked in full first, as a `--file` dump: refused before
    // anything is changed.
    assert!(
        stderr.contains("stdin is cut short") && stderr.contains("nothing was changed"),
        "{stderr}"
    );
}

#[test]
fn a_dump_that_ran_on_another_connection_fails_the_restore() {
    // psql's output reaches berth's markers whatever the service kind: a
    // dump that switched connection, even quietly, never reports success.
    for (label, project) in [
        ("shared", PROJECT.to_owned()),
        (
            "dedicated",
            PROJECT.replace(
                "    preset: postgres\n",
                "    preset: postgres\n    isolation: dedicated\n",
            ),
        ),
    ] {
        let host = Host::new(&format!("switched-{label}"));
        let checkout = host.checkout("a", &project);
        host.ok(&checkout, &["up", "--no-wait"]);
        std::fs::write(checkout.join("dump.sql"), "CREATE TABLE users (id int);\n").expect("dump");
        let restore = bounded_output(
            host.command(
                &checkout,
                &["restore", "--service", "db", "--file", "dump.sql"],
                "",
                "",
            )
            .env("FAKE_SWITCHED", "8"),
        );
        assert!(!restore.status.success(), "{label}");
        assert!(
            text(&restore.stderr).contains("switched database connection"),
            "{label}: {}",
            text(&restore.stderr)
        );
    }
}

#[test]
fn a_shared_backed_service_refuses_cpu_and_memory_caps() {
    // A tenant has no container of its own to cap.
    let host = Host::new("shared-limits");
    let checkout = host.checkout(
        "a",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    cpus: 2\n",
        ),
    );
    let up = bounded_output(&mut host.command(&checkout, &["up", "--no-wait"], "", ""));
    assert!(!up.status.success());
    let stderr = text(&up.stderr);
    assert!(
        stderr.contains("`cpus`") && stderr.contains("isolation: dedicated"),
        "{stderr}"
    );
}

/// The one instance directory of project `shop` under the berth home.
fn instance_dir(host: &Host) -> PathBuf {
    let mut dirs: Vec<_> = std::fs::read_dir(host.base.join("home/instances/shop"))
        .expect("instances")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    dirs.pop().expect("instance")
}

#[test]
fn purging_an_instance_visits_only_the_shared_servers_it_has_tenants_on() {
    // Another checkout's shared server is stopped; this checkout's services
    // are all dedicated, so its purge never starts or locks that server.
    let host = Host::new("purge-own-servers");
    let shared = host.checkout("a", PROJECT);
    host.ok(&shared, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    let dedicated = host.checkout(
        "b",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    );
    host.ok(&dedicated, &["up", "--no-wait"]);
    host.clear();
    let purge = host.ok(&dedicated, &["down", "--purge"]);
    // It says which server it left alone, and why.
    assert!(
        text(&purge.stderr).contains(&format!("Skipped shared server {server}: it is stopped")),
        "{}",
        text(&purge.stderr)
    );
    // Its state may be read; it is never started, run in or changed.
    let calls = host.calls();
    assert!(
        !calls
            .lines()
            .any(|line| line.contains(&server) && !line.starts_with("inspect")),
        "the purge touched {server}: {calls}"
    );
}

#[test]
fn an_instance_without_a_tenant_record_is_purged_from_every_server() {
    // Made before the record existed: its tenants are found by name, even
    // on a stopped server (an unlisted one would be skipped).
    let host = Host::new("purge-legacy");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let mine = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    std::fs::remove_file(instance_dir(&host).join("shared-servers")).expect("legacy instance");
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    let purge = host.berth_with(&a, &["down", "--purge"], &format!("postgres {mine}"));
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert!(
        host.calls()
            .lines()
            .any(|line| line.starts_with("start") && line.contains(&server)),
        "{}",
        host.calls()
    );
    assert!(
        host.sql()
            .contains(&format!("DROP DATABASE IF EXISTS \"{mine}\" WITH (FORCE);")),
        "its tenant was dropped on {server}: {}",
        host.sql()
    );
}

#[test]
fn a_dry_run_whole_purge_names_the_tenant_drop_and_runs_nothing() {
    let host = Host::new("purge-dry-run");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.clear();
    let preview = host.ok(&a, &["--dry-run", "down", "--purge"]);
    let shown = format!("{}{}", text(&preview.stdout), text(&preview.stderr));
    assert!(shown.contains("Drop every tenant of"), "{shown}");
    assert!(
        !host
            .calls()
            .lines()
            .any(|line| line.starts_with("exec") && line.contains(&server)),
        "{}",
        host.calls()
    );
}

#[test]
fn a_listed_server_whose_state_is_unknown_fails_the_purge_and_keeps_it_findable() {
    // The instance's db moved to a dedicated server, so only the record
    // names the shared one; an engine that does not answer for it must not
    // count as no server.
    let host = Host::new("purge-listed-unknown");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::write(
        a.join(".berth.yaml"),
        PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    )
    .expect("dedicated");
    let state = instance_dir(&host);
    std::fs::write(host.base.join(format!("state/inspect-fails-{server}")), "").expect("fail");
    host.clear();
    let purge = host.berth(&a, &["down", "--purge"]);
    assert!(!purge.status.success(), "{}", text(&purge.stderr));
    assert!(!host.sql().contains("DROP"), "{}", host.sql());
    assert_eq!(
        std::fs::read_to_string(state.join("shared-servers")).expect("record"),
        "*\n"
    );
}

#[test]
fn every_server_mark_survives_a_later_up_and_service_purge() {
    let host = Host::new("purge-mark-survives");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let state = instance_dir(&host);
    let failed = bounded_output(
        host.command(&a, &["down", "--purge"], "", "")
            .env("FAKE_SERVER", "norole"),
    );
    assert!(!failed.status.success());
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&a, &["down", "--purge", "--service", "db"]);
    let record = std::fs::read_to_string(state.join("shared-servers")).expect("record");
    assert!(record.lines().any(|line| line == "*"), "{record:?}");
}

#[test]
fn a_purge_that_leaves_tenants_marks_every_server_for_the_next() {
    // Its state goes, but the tenants it could not drop stay findable.
    let host = Host::new("purge-failed-tenants");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let state = instance_dir(&host);
    let purge = bounded_output(
        host.command(&a, &["down", "--purge"], "", "")
            .env("FAKE_SERVER", "norole"),
    );
    assert!(!purge.status.success(), "{}", text(&purge.stderr));
    assert_eq!(
        std::fs::read_to_string(state.join("shared-servers")).expect("record"),
        "*\n",
        "the next purge visits every server"
    );
    assert!(
        !state.join("lease.json").exists(),
        "the purged instance gets no lease back"
    );
}

#[test]
fn a_running_server_the_record_misses_without_a_tenant_is_left_alone() {
    // Another checkout's server runs, held by another berth process; this
    // dedicated instance has no tenant there, so its purge only reads it:
    // it neither waits on its lock nor drops anything on it.
    let host = Host::new("purge-running-unlisted");
    let shared = host.checkout("a", PROJECT);
    host.ok(&shared, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let dedicated = host.checkout(
        "b",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    );
    host.ok(&dedicated, &["up", "--no-wait"]);
    host.clear();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(host.base.join(format!("home/locks/shared-{server}.lock")))
        .expect("server lock");
    held.lock().expect("hold the server's lock");
    host.ok(&dedicated, &["down", "--purge"]);
    drop(held);
    let sql = host.sql();
    assert!(
        !sql.contains("DROP"),
        "the purge dropped something on {server}: {sql}"
    );
}

#[test]
fn a_running_server_the_record_misses_that_does_not_answer_is_skipped() {
    // Another checkout's server refuses connections (restarting, say): the
    // dedicated instance's purge neither waits for it nor fails over it.
    let host = Host::new("purge-unanswering-unlisted");
    let shared = host.checkout("a", PROJECT);
    host.ok(&shared, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let dedicated = host.checkout(
        "b",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    );
    host.ok(&dedicated, &["up", "--no-wait"]);
    std::fs::write(host.base.join("state/refuse"), "").expect("refuse");
    let purge = bounded_output(
        host.command(&dedicated, &["down", "--purge"], "", "")
            .env("FAKE_SERVER", "refusing"),
    );
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert!(
        text(&purge.stderr).contains(&format!(
            "Skipped shared server {server}: it did not answer"
        )),
        "{}",
        text(&purge.stderr)
    );
}

#[test]
fn a_server_the_record_misses_whose_state_is_unknown_is_skipped_and_named() {
    let host = Host::new("purge-unknown-unlisted");
    let shared = host.checkout("a", PROJECT);
    host.ok(&shared, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let dedicated = host.checkout(
        "b",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    );
    host.ok(&dedicated, &["up", "--no-wait"]);
    std::fs::write(host.base.join(format!("state/inspect-fails-{server}")), "").expect("fail");
    let purge = host.ok(&dedicated, &["down", "--purge"]);
    assert!(
        text(&purge.stderr).contains(&format!(
            "Skipped shared server {server}: the engine did not report its state"
        )),
        "{}",
        text(&purge.stderr)
    );
}

#[test]
fn a_server_left_with_no_tenant_of_the_instance_leaves_its_record() {
    // Purging the instance's only shared service drops its tenant: a later
    // purge of the rest no longer visits (or starts) that server.
    let host = Host::new("purge-forget-server");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let record = instance_dir(&host).join("shared-servers");
    assert_eq!(
        std::fs::read_to_string(&record).expect("record"),
        format!("{server}\n")
    );
    host.ok(&a, &["down", "--purge", "--service", "db"]);
    assert_eq!(std::fs::read_to_string(&record).expect("record"), "");
    // While another tenant of the instance stays there (another runtime
    // environment's), the server stays on the record.
    host.ok(&a, &["up", "--no-wait"]);
    let mine = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let staging = format!("{}__db__staging", mine.trim_end_matches("__db"));
    let purge = host.berth_with(
        &a,
        &["down", "--purge", "--service", "db"],
        &format!("postgres {staging}"),
    );
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert_eq!(
        std::fs::read_to_string(&record).expect("record"),
        format!("{server}\n")
    );
}

#[test]
fn after_a_failed_purge_the_retry_visits_and_starts_every_server() {
    // The `*` mark brings a stopped server, holding the tenants the purge
    // could not drop, back into the retry.
    let host = Host::new("purge-retry-every-server");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let mine = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    let failed = bounded_output(
        host.command(&a, &["down", "--purge"], "", "")
            .env("FAKE_SERVER", "norole"),
    );
    assert!(!failed.status.success(), "{}", text(&failed.stderr));
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    let retry = host.berth_with(&a, &["down", "--purge"], &format!("postgres {mine}"));
    assert!(retry.status.success(), "{}", text(&retry.stderr));
    assert!(
        host.calls()
            .lines()
            .any(|line| line.starts_with("start") && line.contains(&server)),
        "the stopped server was started: {}",
        host.calls()
    );
    assert!(
        host.sql()
            .contains(&format!("DROP DATABASE IF EXISTS \"{mine}\" WITH (FORCE);")),
        "{}",
        host.sql()
    );
}

#[test]
fn a_reset_keeps_its_server_on_the_record_and_never_writes_it() {
    // A reset drops and provisions again: the server stays listed, and a
    // record that cannot be written does not stop it.
    let host = Host::new("reset-record");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let state = instance_dir(&host);
    std::fs::write(
        a.join("dump.sql"),
        "CREATE TABLE restored_marker (id int);\n",
    )
    .expect("dump");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).expect("read-only");
    let restore = host.berth_with(
        &a,
        &[
            "restore",
            "--service",
            "db",
            "--reset",
            "--file",
            "dump.sql",
        ],
        "",
    );
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).expect("writable");
    assert!(restore.status.success(), "{}", text(&restore.stderr));
    assert!(host.sql().contains("CREATE DATABASE"), "{}", host.sql());
    assert_eq!(
        std::fs::read_to_string(state.join("shared-servers")).expect("record"),
        format!("{server}\n")
    );
}

#[test]
fn a_whole_purge_leaves_a_server_its_record_dropped_alone() {
    let host = Host::new("purge-whole-after-forget");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.ok(&a, &["down", "--purge", "--service", "db"]);
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    host.ok(&a, &["down", "--purge"]);
    assert!(
        !host
            .calls()
            .lines()
            .any(|line| line.starts_with("start") && line.contains(&server)),
        "{}",
        host.calls()
    );
}

#[test]
fn a_reset_fails_when_the_engine_does_not_say_whether_its_server_exists() {
    // Taking an unanswered inspect for a missing server would keep the old
    // data under a reset that reports success.
    let host = Host::new("reset-unknown-server");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    std::fs::write(
        a.join("dump.sql"),
        "CREATE TABLE restored_marker (id int);\n",
    )
    .expect("dump");
    std::fs::write(host.base.join(format!("state/inspect-fails-{server}")), "").expect("fail");
    host.clear();
    let restore = host.berth_with(
        &a,
        &[
            "restore",
            "--service",
            "db",
            "--reset",
            "--file",
            "dump.sql",
        ],
        "",
    );
    assert!(!restore.status.success());
    assert!(
        !host.sql().contains("DROP"),
        "nothing was dropped: {}",
        host.sql()
    );
    // A purge of the service fails too, instead of reporting a drop it never
    // made.
    let purge = host.berth_with(&a, &["down", "--purge", "--service", "db"], "");
    assert!(!purge.status.success(), "{}", text(&purge.stderr));
}

#[test]
fn a_stop_during_another_alias_change_still_detaches_its_service() {
    // Changing aliases disconnects, then reconnects: a stop that looked
    // meanwhile would see the server detached and leave its own alias on.
    let host = Host::new("alias-change-race");
    let a = host.checkout("a", TWO_DATABASES);
    host.ok(&a, &["up", "--no-wait"]);
    assert_eq!(attachment(&host, "berth_shop_a-"), ["analytics", "db"]);
    let server = host.servers().pop().expect("server");
    let instance = instance_dir(&host)
        .file_name()
        .expect("instance")
        .to_string_lossy()
        .into_owned();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            host.base
                .join(format!("home/locks/aliases_shop_{instance}.lock")),
        )
        .expect("alias lock");
    held.lock().expect("hold the instance's alias lock");
    // Mid-change: disconnected, about to reconnect with both names.
    let attached = std::fs::read_dir(host.base.join("state/att").join(&server))
        .expect("attachments")
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("berth_shop_a-")
        })
        .expect("instance network")
        .path();
    let aliases = std::fs::read(&attached).expect("aliases");
    std::fs::remove_file(&attached).expect("disconnect");
    let log = host.base.join("state/stop-stderr");
    let mut child = host
        .command(&a, &["stop", "--service", "analytics"], "", "")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::fs::File::create(&log).expect("stderr log"))
        .spawn()
        .expect("stop");
    // Until the stop waits on the alias lock (or exits without it, as it
    // would without the lock): the change goes on only then.
    let notice = format!("the aliases_shop_{instance} lock, which another berth process holds");
    let started = std::time::Instant::now();
    let exited_early = loop {
        if child.try_wait().expect("poll stop").is_some() {
            break true;
        }
        if std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains(&notice)
        {
            break false;
        }
        let stuck = started.elapsed() > std::time::Duration::from_secs(60);
        if stuck {
            drop(child.kill());
            drop(child.wait());
        }
        assert!(
            !stuck,
            "the stop neither waited nor exited: {}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    std::fs::write(&attached, aliases).expect("reconnect");
    drop(held);
    let status = child.wait().expect("stop exits");
    let stderr = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !exited_early,
        "the stop did not wait for the change: {stderr}"
    );
    assert!(status.success(), "{stderr}");
    assert_eq!(attachment(&host, "berth_shop_a-"), ["db"]);
}

#[test]
fn a_stop_during_an_up_s_reconnect_waits_for_it_and_detaches() {
    // `up --service analytics` reconnects the server with both names; a stop
    // of db meanwhile must wait for that, then take its own name off.
    let host = Host::new("alias-attach-race");
    let a = host.checkout("a", TWO_DATABASES);
    host.ok(&a, &["up", "--no-wait"]);
    host.ok(&a, &["stop", "--service", "analytics"]);
    assert_eq!(attachment(&host, "berth_shop_a-"), ["db"]);
    let instance = instance_dir(&host)
        .file_name()
        .expect("instance")
        .to_string_lossy()
        .into_owned();
    let hold = host.base.join("state/connect-hold");
    let waiting = host.base.join("state/connect-waiting");
    std::fs::write(&hold, "").expect("hold");
    let quiet = |command: &mut Command, log: &Path| {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(log).expect("log"))
            .spawn()
            .expect("spawn")
    };
    let up_log = host.base.join("state/up-stderr");
    let mut up = quiet(
        &mut host.command(&a, &["up", "--no-wait", "--service", "analytics"], "", ""),
        &up_log,
    );
    let started = std::time::Instant::now();
    let until = |done: &dyn Fn() -> bool, what: &str, children: &mut [&mut std::process::Child]| {
        while !done() {
            let stuck = started.elapsed() > std::time::Duration::from_secs(60);
            if stuck {
                drop(std::fs::remove_file(&hold));
                for child in children.iter_mut() {
                    drop(child.kill());
                    drop(child.wait());
                }
            }
            assert!(!stuck, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    until(
        &|| waiting.exists(),
        "up never reached its reconnect",
        &mut [&mut up],
    );
    let stop_log = host.base.join("state/stop-stderr");
    let mut stop = quiet(
        &mut host.command(&a, &["stop", "--service", "db"], "", ""),
        &stop_log,
    );
    let notice = format!("the aliases_shop_{instance} lock, which another berth process holds");
    // The stop waits on the alias lock, or (without the up holding it)
    // exits at once, having seen the server detached.
    loop {
        if std::fs::read_to_string(&stop_log)
            .unwrap_or_default()
            .contains(&notice)
        {
            break;
        }
        let failed =
            stop.try_wait().expect("poll stop").is_some() || started.elapsed().as_secs() > 60;
        if failed {
            drop(stop.kill());
            drop(stop.wait());
            drop(std::fs::remove_file(&hold));
            drop(up.kill());
            drop(up.wait());
        }
        assert!(
            !failed,
            "the stop did not wait for the up's alias change: {}",
            std::fs::read_to_string(&stop_log).unwrap_or_default()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::fs::remove_file(&hold).expect("release");
    let up_status = up.wait().expect("up exits");
    let stop_status = stop.wait().expect("stop exits");
    assert!(
        up_status.success(),
        "{}",
        std::fs::read_to_string(&up_log).unwrap_or_default()
    );
    assert!(
        stop_status.success(),
        "{}",
        std::fs::read_to_string(&stop_log).unwrap_or_default()
    );
    assert_eq!(attachment(&host, "berth_shop_a-"), ["analytics"]);
}

#[test]
fn a_redis_server_leaves_the_record_only_with_the_instance_s_last_user() {
    let host = Host::new("purge-forget-redis");
    let a = host.checkout("a", REDIS_PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let record = instance_dir(&host).join("shared-servers");
    let server = std::fs::read_to_string(&record)
        .expect("record")
        .trim()
        .to_owned();
    assert!(server.starts_with("berth_shared_redis_"), "{server}");
    let mine = env_json(&host, &a)["environment"]["REDIS_USERNAME"]
        .as_str()
        .expect("user")
        .to_owned();
    let staging = format!("{}__cache__staging", mine.trim_end_matches("__cache"));
    // Another runtime environment's user stays: so does the server.
    let kept = host.berth_with(
        &a,
        &["down", "--purge", "--service", "cache"],
        &format!("default {staging}"),
    );
    assert!(kept.status.success(), "{}", text(&kept.stderr));
    assert_eq!(
        std::fs::read_to_string(&record).expect("record"),
        format!("{server}\n")
    );
    host.ok(&a, &["up", "--no-wait"]);
    let gone = host.berth_with(&a, &["down", "--purge", "--service", "cache"], "default");
    assert!(gone.status.success(), "{}", text(&gone.stderr));
    assert_eq!(std::fs::read_to_string(&record).expect("record"), "");
}

#[test]
fn a_dead_runs_half_made_instance_directory_is_swept() {
    // A run that died between making an instance directory and moving it
    // into place leaves it in `instance-staging`; the next new instance
    // removes it (a live run's stays).
    let host = Host::new("staging-sweep");
    let staging = host.base.join("home/instance-staging");
    let dead = staging.join(format!("shop_old-1a2b3c_{}", i32::MAX));
    let live = staging.join(format!("shop_other-1a2b3c_{}", std::process::id()));
    for dir in [&dead, &live] {
        std::fs::create_dir_all(dir).expect("staging");
    }
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    assert!(!dead.exists(), "the dead run's staging was swept");
    assert!(live.exists(), "a live run's staging stays");
}

#[test]
fn a_mysql_server_leaves_the_record_only_with_the_instance_s_last_user() {
    // A MySQL tenant's user is not named like its schema: it alone keeps
    // the server on the record.
    let host = Host::new("purge-forget-mysql");
    let a = host.checkout("a", &mysql_project("mysql:8.4"));
    host.ok(&a, &["up", "--no-wait"]);
    let record = instance_dir(&host).join("shared-servers");
    let server = std::fs::read_to_string(&record)
        .expect("record")
        .trim()
        .to_owned();
    assert!(server.starts_with("berth_shared_mysql_"), "{server}");
    let user = env_json(&host, &a)["environment"]["DB_USERNAME"]
        .as_str()
        .expect("user")
        .to_owned();
    let kept = host.berth_with(&a, &["down", "--purge", "--service", "db"], &user);
    assert!(kept.status.success(), "{}", text(&kept.stderr));
    assert_eq!(
        std::fs::read_to_string(&record).expect("record"),
        format!("{server}\n")
    );
    host.ok(&a, &["up", "--no-wait"]);
    let gone = host.berth_with(&a, &["down", "--purge", "--service", "db"], "");
    assert!(gone.status.success(), "{}", text(&gone.stderr));
    assert_eq!(std::fs::read_to_string(&record).expect("record"), "");
}

#[test]
fn purges_after_the_record_dropped_a_server_neither_start_nor_wait_on_it() {
    let host = Host::new("purge-after-forget");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    host.ok(&a, &["down", "--purge", "--service", "db"]);
    // A second purge of the service, with the server stopped: not started.
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    let again = host.ok(&a, &["down", "--purge", "--service", "db"]);
    assert!(
        !host
            .calls()
            .lines()
            .any(|line| line.starts_with("start") && line.contains(&server)),
        "{}",
        host.calls()
    );
    assert!(
        text(&again.stderr).contains(&format!("Skipped shared server {server}: it is stopped")),
        "{}",
        text(&again.stderr)
    );
    // Running, and held by another berth process: the whole purge does not
    // wait for it.
    std::fs::remove_file(host.base.join("state").join(format!("stopped-{server}"))).expect("start");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(host.base.join(format!("home/locks/shared-{server}.lock")))
        .expect("server lock");
    held.lock().expect("hold the server's lock");
    host.ok(&a, &["down", "--purge"]);
    drop(held);
}

#[test]
fn a_running_server_is_checked_for_tenants_its_record_misses() {
    // A lease removed by hand makes a new, empty record for an old instance:
    // its tenants on a running server are still found and dropped.
    let host = Host::new("purge-empty-record");
    let a = host.checkout("a", PROJECT);
    host.ok(&a, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let mine = env_json(&host, &a)["environment"]["DB_DATABASE"]
        .as_str()
        .expect("tenant")
        .to_owned();
    std::fs::write(instance_dir(&host).join("shared-servers"), "").expect("empty record");
    host.clear();
    // A tenant no current service owns (another runtime environment's):
    // only the instance-wide check finds it.
    let staging = format!("{}__db__staging", mine.trim_end_matches("__db"));
    let purge = host.berth_with(
        &a,
        &["down", "--purge"],
        &format!("postgres {mine} {staging}"),
    );
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let sql = host.sql();
    assert!(
        sql.contains(&format!(
            "DROP DATABASE IF EXISTS \"{staging}\" WITH (FORCE);"
        )),
        "its tenant was dropped on {server}: {sql}"
    );
}

#[test]
fn a_new_instances_record_starts_with_its_state_whatever_made_it() {
    // `env` makes b's state (no lease) before its first `up`: its record is
    // there from the start and empty, so its purge leaves a's stopped
    // server alone.
    let host = Host::new("purge-env-first");
    let shared = host.checkout("a", PROJECT);
    host.ok(&shared, &["up", "--no-wait"]);
    let server = host.servers().pop().expect("server");
    let dedicated = host.checkout(
        "b",
        &PROJECT.replace(
            "    preset: postgres\n",
            "    preset: postgres\n    isolation: dedicated\n",
        ),
    );
    host.ok(&dedicated, &["env", "--json"]);
    let state = std::fs::read_dir(host.base.join("home/instances/shop"))
        .expect("instances")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| !path.join("lease.json").exists())
        .expect("b's state");
    assert_eq!(
        std::fs::read_to_string(state.join("shared-servers")).expect("record"),
        ""
    );
    host.ok(&dedicated, &["up", "--no-wait"]);
    std::fs::write(
        host.base.join("state").join(format!("stopped-{server}")),
        b"",
    )
    .expect("stop");
    host.clear();
    host.ok(&dedicated, &["down", "--purge"]);
    assert!(
        !host
            .calls()
            .lines()
            .any(|line| line.contains(&server) && !line.starts_with("inspect")),
        "{}",
        host.calls()
    );
}

#[test]
fn a_tenant_that_failed_to_provision_is_still_on_the_record() {
    // Its role may exist without its database: the purge visits the server.
    let host = Host::new("purge-failed-provision");
    let a = host.checkout("a", PROJECT);
    std::fs::write(host.base.join("state/tenant-fails"), b"").expect("fail provisioning");
    let up = bounded_output(&mut host.command(&a, &["up", "--no-wait"], "", ""));
    assert!(!up.status.success());
    let server = host.servers().pop().expect("server");
    assert_eq!(
        std::fs::read_to_string(instance_dir(&host).join("shared-servers")).expect("record"),
        format!("{server}\n")
    );
    std::fs::remove_file(host.base.join("state/tenant-fails")).expect("heal");
    host.ok(&a, &["down", "--purge"]);
    let locks: Vec<_> = std::fs::read_dir(host.base.join("home/locks"))
        .expect("locks")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("tenants_") || name.starts_with("aliases_"))
        .collect();
    assert!(
        locks.is_empty(),
        "the purge removed its tenant-record and alias locks: {locks:?}"
    );
}

#[test]
fn purging_every_service_of_an_instance_removes_its_lease() {
    let host = Host::new("purge-every-service");
    let a = host.checkout(
        "a",
        "schema_version: 9\nproject: shop\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n",
    );
    host.ok(&a, &["up", "--no-wait"]);
    let state = instance_dir(&host);
    assert!(state.join("lease.json").exists());
    host.ok(&a, &["down", "--purge", "--service", "db"]);
    assert!(!state.exists(), "the lease and the rest of its state went");
}

#[test]
fn a_whole_dump_on_stdin_restores_for_either_service_kind() {
    use std::io::Write;
    for (label, project) in [
        ("shared", PROJECT.to_owned()),
        (
            "dedicated",
            PROJECT.replace(
                "    preset: postgres\n",
                "    preset: postgres\n    isolation: dedicated\n",
            ),
        ),
    ] {
        let host = Host::new(&format!("stdin-whole-{label}"));
        let checkout = host.checkout("a", &project);
        host.ok(&checkout, &["up", "--no-wait"]);
        let mut child = host
            .command(&checkout, &["restore", "--service", "db"], "", "")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(b"CREATE TABLE users (id int);\n")
            .expect("write");
        let restore = child.wait_with_output().expect("restore");
        assert!(
            restore.status.success(),
            "{label}: {}",
            text(&restore.stderr)
        );
    }
}

#[test]
fn a_tenant_whose_login_is_refused_is_never_reset() {
    let host = Host::new("tenant-login");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    std::fs::write(checkout.join("dump.sql"), "CREATE TABLE users (id int);\n").expect("dump");
    host.clear();
    let restore = bounded_output(
        host.command(
            &checkout,
            &[
                "restore",
                "--service",
                "db",
                "--reset",
                "--file",
                "dump.sql",
            ],
            "",
            "",
        )
        .env("FAKE_TENANT_LOGIN_FAILS", "1"),
    );
    let stderr = text(&restore.stderr);
    assert!(!restore.status.success(), "{stderr}");
    assert!(
        stderr.contains("refuses berth's database client, so nothing was changed")
            && stderr.contains("password authentication failed"),
        "{stderr}"
    );
    let calls = format!("{}{}", host.calls(), host.sql());
    assert!(!calls.contains("DROP DATABASE"), "{calls}");
}

#[test]
fn a_dry_run_restore_never_prints_the_dump() {
    let host = Host::new("dry-run-restore");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    std::fs::write(
        checkout.join("dump.sql"),
        "CREATE TABLE secret_rows (id int);\n",
    )
    .expect("dump");
    let restore = host.berth(
        &checkout,
        &[
            "--dry-run",
            "restore",
            "--service",
            "db",
            "--file",
            "dump.sql",
        ],
    );
    assert!(restore.status.success(), "{}", text(&restore.stderr));
    assert!(
        !text(&restore.stdout).contains("secret_rows"),
        "{}",
        text(&restore.stdout)
    );
}

#[test]
fn a_reset_restore_that_cannot_reach_the_tenant_drops_nothing() {
    let host = Host::new("reset-unreachable");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    // `down` detaches the server; its data stays.
    host.ok(&checkout, &["down"]);
    std::fs::write(checkout.join("ok.sql"), "CREATE TABLE t (id int);\n").expect("dump");
    host.clear();
    let restore = host.berth(
        &checkout,
        &["restore", "--service", "db", "--reset", "--file", "ok.sql"],
    );
    assert!(!restore.status.success(), "{}", text(&restore.stderr));
    assert!(
        text(&restore.stderr).contains(" up` first"),
        "{}",
        text(&restore.stderr)
    );
    assert!(!host.sql().contains("DROP DATABASE"), "{}", host.sql());
}

#[test]
fn a_dump_that_fails_part_way_leaves_nothing_behind() {
    let host = Host::new("dump-fails");
    let checkout = host.checkout("a", PROJECT);
    host.ok(&checkout, &["up", "--no-wait"]);
    std::fs::write(checkout.join("backup.sql"), "previous dump").expect("old dump");
    let failing = |args: &[&str]| {
        bounded_output(
            host.command(&checkout, args, "", "")
                .env("FAKE_DUMP_OUT", "CREATE TABLE half")
                .env("FAKE_CLIENT_EXIT", "1"),
        )
    };
    let to_file = failing(&["dump", "--service", "db", "--file", "backup.sql"]);
    assert!(!to_file.status.success(), "{}", text(&to_file.stderr));
    assert_eq!(
        std::fs::read_to_string(checkout.join("backup.sql")).expect("old"),
        "previous dump",
        "a failed dump keeps the previous one"
    );
    let partials = std::fs::read_dir(&checkout)
        .expect("checkout")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".berth-partial")
        })
        .count();
    assert_eq!(partials, 0, "no partial dump is left");
    let to_stdout = failing(&["dump", "--service", "db", "--stdout"]);
    assert!(!to_stdout.status.success(), "{}", text(&to_stdout.stderr));
    assert!(
        to_stdout.stdout.is_empty(),
        "a failed dump never reaches a pipe"
    );
}

#[test]
fn a_failed_image_probe_reports_the_engines_error_not_a_missing_pgdata() {
    let host = Host::new("probe-fails");
    let checkout = host.checkout("a", PROJECT);
    let up = bounded_output(
        host.command(&checkout, &["up", "--no-wait"], "", "")
            .env("FAKE_PROBE_FAIL", "1"),
    );
    assert!(!up.status.success(), "{}", text(&up.stderr));
    let stderr = text(&up.stderr);
    assert!(
        stderr.contains("parent snapshot does not exist"),
        "{stderr}"
    );
    assert!(!stderr.contains("does not declare PGDATA"), "{stderr}");
}
