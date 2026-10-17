//! MySQL wrapper script builders.

/// Returns shell script that installs a `mysqldump` wrapper in-container.
pub(super) fn install_wrapper_script(sql_client_flavor: &str) -> String {
    format!(
        r#"cat <<'EOF' >/usr/local/bin/mysqldump
#!/usr/bin/env bash
set -euo pipefail

BERTH_SQL_CLIENT_FLAVOR={sql_client_flavor}
real_mysqldump="/usr/bin/mysqldump"
if [[ ! -x "$real_mysqldump" ]]; then
  real_mysqldump="$(command -v mariadb-dump || command -v mysqldump)"
fi

sanitized=()
result_file=""
for arg in "$@"; do
  case "$arg" in
    --column-statistics=0|--set-gtid-purged=OFF)
      continue
      ;;
    --result-file=*)
      result_file="${{arg#--result-file=}}"
      ;;
  esac
  sanitized+=("$arg")
done

stderr_file="$(mktemp)"
if "$real_mysqldump" "${{sanitized[@]}}" 2>"$stderr_file"; then
  if [[ "$BERTH_SQL_CLIENT_FLAVOR" == "mysql" && -n "$result_file" && -f "$result_file" ]]; then
    sed -i '/^\/\*M\!.*\*\/;*$/d' "$result_file"
  fi
  rm -f "$stderr_file"
  exit 0
fi
status=$?

if grep -qi "self-signed certificate in certificate chain" "$stderr_file"; then
  if "$real_mysqldump" --help 2>/dev/null | grep -q -- "--ssl-mode"; then
    "$real_mysqldump" --ssl-mode=DISABLED "${{sanitized[@]}}"
  else
    "$real_mysqldump" --skip-ssl "${{sanitized[@]}}"
  fi
  retry_status=$?
  if [[ "$BERTH_SQL_CLIENT_FLAVOR" == "mysql" && -n "$result_file" && -f "$result_file" ]]; then
    sed -i '/^\/\*M\!.*\*\/;*$/d' "$result_file"
  fi
  rm -f "$stderr_file"
  exit "$retry_status"
fi

cat "$stderr_file" >&2
rm -f "$stderr_file"
exit "$status"
EOF
chmod +x /usr/local/bin/mysqldump"#
    )
}

/// Returns shell script that installs a `mysql` wrapper in-container.
pub(super) fn install_mysql_wrapper_script() -> &'static str {
    r#"cat <<'EOF' >/usr/local/bin/mysql
#!/usr/bin/env bash
set -euo pipefail

real_mysql="/usr/bin/mysql"
if [[ ! -x "$real_mysql" ]]; then
  real_mysql="$(command -v mariadb || command -v mysql)"
fi

stderr_file="$(mktemp)"
if "$real_mysql" "$@" 2>"$stderr_file"; then
  rm -f "$stderr_file"
  exit 0
fi
status=$?

if grep -qi "self-signed certificate in certificate chain" "$stderr_file"; then
  if "$real_mysql" --help 2>/dev/null | grep -q -- "--ssl-mode"; then
    "$real_mysql" --ssl-mode=DISABLED "$@"
  else
    "$real_mysql" --skip-ssl "$@"
  fi
  retry_status=$?
  rm -f "$stderr_file"
  exit "$retry_status"
fi

cat "$stderr_file" >&2
rm -f "$stderr_file"
exit "$status"
EOF
chmod +x /usr/local/bin/mysql"#
}
