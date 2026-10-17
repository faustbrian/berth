//! Parses the database and role listings the engines' listing queries
//! print (`d\tname\towner` and `r\tname\t`).

use anyhow::Result;

/// Splits a listing into the databases to drop (named by `database` or
/// owned by a role `role` matches) and the roles to drop.
///
/// # Errors
///
/// Returns an error for a line a listing query cannot produce.
pub(super) fn select_names(
    listing: &str,
    database: &impl Fn(&str) -> bool,
    role: &impl Fn(&str) -> bool,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut databases = Vec::new();
    let mut roles = Vec::new();
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        match (fields.next(), fields.next(), fields.next()) {
            (Some("d"), Some(name), Some(owner)) => {
                if database(name) || (!owner.is_empty() && role(owner)) {
                    databases.push(name.to_owned());
                }
            }
            (Some("r"), Some(name), _) => {
                if role(name) {
                    roles.push(name.to_owned());
                }
            }
            _ => anyhow::bail!("unexpected database listing line '{line}'"),
        }
    }
    Ok((databases, roles))
}

#[cfg(test)]
mod tests {
    use super::select_names;

    #[test]
    fn databases_a_matching_role_owns_are_selected_with_it() {
        let listing = "d\tshop__a__db\tshop__a__db\nd\toutside\tshop__a__db\nd\tother\tother\nd\tlone\t\nr\tshop__a__db\t\nr\tother\t\n";
        let ours = |name: &str| name.starts_with("shop__a");
        let (databases, roles) = select_names(listing, &ours, &ours).expect("parse");
        assert_eq!(databases, ["shop__a__db", "outside"]);
        assert_eq!(roles, ["shop__a__db"]);
        assert!(select_names("x\ty", &ours, &ours).is_err());
    }
}
