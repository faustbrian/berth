//! Shared dependency ordering helpers.

use anyhow::Result;
use std::collections::HashSet;

/// Topologically orders selected names by dependency relation.
///
/// A cycle fails with `cycle_error` applied to the whole path, such as
/// `app -> worker -> app` (FI-29), so the config can be fixed in one look.
pub(crate) fn order_dependency_names<FDep, FCycle>(
    selected_roots: &[String],
    dependencies_for: FDep,
    cycle_error: FCycle,
) -> Result<Vec<String>>
where
    FDep: Fn(&str) -> Result<Vec<String>>,
    FCycle: Fn(&str) -> String,
{
    let mut ordered = Vec::new();
    let mut visiting: Vec<String> = Vec::new();
    let mut visited = HashSet::new();

    fn visit<FDep, FCycle>(
        current: &str,
        dependencies_for: &FDep,
        cycle_error: &FCycle,
        visiting: &mut Vec<String>,
        visited: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) -> Result<()>
    where
        FDep: Fn(&str) -> Result<Vec<String>>,
        FCycle: Fn(&str) -> String,
    {
        if visited.contains(current) {
            return Ok(());
        }
        if let Some(start) = visiting.iter().position(|name| name == current) {
            let mut path: Vec<&str> = visiting[start..].iter().map(String::as_str).collect();
            path.push(current);
            anyhow::bail!("{}", cycle_error(&path.join(" -> ")));
        }
        visiting.push(current.to_owned());

        for dependency in dependencies_for(current)? {
            visit(
                dependency.as_str(),
                dependencies_for,
                cycle_error,
                visiting,
                visited,
                ordered,
            )?;
        }

        visiting.pop();
        visited.insert(current.to_owned());
        ordered.push(current.to_owned());
        Ok(())
    }

    for name in selected_roots {
        visit(
            name,
            &dependencies_for,
            &cycle_error,
            &mut visiting,
            &mut visited,
            &mut ordered,
        )?;
    }

    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::order_dependency_names;

    fn order(edges: &[(&str, &[&str])], roots: &[&str]) -> anyhow::Result<Vec<String>> {
        let roots: Vec<String> = roots.iter().map(|root| (*root).to_owned()).collect();
        order_dependency_names(
            &roots,
            |current| {
                Ok(edges
                    .iter()
                    .find(|(name, _)| *name == current)
                    .map(|(_, deps)| deps.iter().map(|dep| (*dep).to_owned()).collect())
                    .unwrap_or_default())
            },
            |path| format!("circular dependency: {path}"),
        )
    }

    #[test]
    fn dependencies_come_first() {
        let ordered =
            order(&[("app", &["db", "cache"]), ("cache", &["db"])], &["app"]).expect("order");
        assert_eq!(ordered, vec!["db", "cache", "app"]);
    }

    #[test]
    fn a_cycle_names_its_whole_path() {
        let error = order(
            &[
                ("app", &["db", "worker"]),
                ("worker", &["queue"]),
                ("queue", &["worker"]),
            ],
            &["app"],
        )
        .expect_err("cycle");
        assert_eq!(
            error.to_string(),
            "circular dependency: worker -> queue -> worker"
        );
        let error = order(&[("app", &["app"])], &["app"]).expect_err("self");
        assert_eq!(error.to_string(), "circular dependency: app -> app");
    }
}
