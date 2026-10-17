//! display about style module.
//!
//! Contains display about style logic used by Berth command workflows.

use std::borrow::Cow;

const DEFAULT_LINE_WIDTH: usize = 120;
const MIN_DOT_PADDING: usize = 4;

pub(crate) struct AboutRow<'a> {
    label: Cow<'a, str>,
    value: Cow<'a, str>,
    value_width: usize,
}

impl<'a> AboutRow<'a> {
    pub(crate) fn plain(label: impl Into<Cow<'a, str>>, value: impl Into<Cow<'a, str>>) -> Self {
        let value = value.into();
        Self {
            label: label.into(),
            value_width: value.chars().count(),
            value,
        }
    }

    pub(crate) fn colored(
        label: impl Into<Cow<'a, str>>,
        plain_value: impl AsRef<str>,
        rendered_value: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            label: label.into(),
            value: rendered_value.into(),
            value_width: plain_value.as_ref().chars().count(),
        }
    }
}

pub(crate) fn print_section(title: &str, rows: &[AboutRow<'_>]) {
    print_section_with_title(title, title, rows);
}

pub(crate) fn print_section_with_title(
    title_plain: &str,
    title_rendered: &str,
    rows: &[AboutRow<'_>],
) {
    let line_width = line_width();
    print_title_line(title_plain, title_rendered, line_width);

    for row in rows {
        print_row(row, line_width);
    }
    println!();
}

fn print_title_line(title_plain: &str, title_rendered: &str, line_width: usize) {
    let left = format!("  {title_rendered}");
    let left_width = format!("  {title_plain}").chars().count();
    let dots = line_width.saturating_sub(left_width).max(MIN_DOT_PADDING);
    println!("{left} {}", ".".repeat(dots));
}

fn print_row(row: &AboutRow<'_>, line_width: usize) {
    let left = format!("  {}", row.label);
    let left_width = left.chars().count();
    let dots = dot_count(left_width, row.value_width, line_width);
    println!("{left} {} {}", ".".repeat(dots), row.value);
}

fn dot_count(left_width: usize, value_width: usize, line_width: usize) -> usize {
    line_width
        .saturating_sub(left_width.saturating_add(value_width).saturating_add(1))
        .max(MIN_DOT_PADDING)
}

fn line_width() -> usize {
    let from_env = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 80);
    from_env.unwrap_or(DEFAULT_LINE_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::{AboutRow, dot_count, line_width, print_row, print_section};
    use std::env;

    #[test]
    fn line_width_respects_env_or_default() {
        let expected = env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value >= 80)
            .unwrap_or(120);

        assert_eq!(line_width(), expected);
    }

    #[test]
    fn dot_count_has_minimum_padding() {
        assert_eq!(dot_count(200, 10, 100), 4);
        assert!(dot_count(10, 10, 30) >= 4);
    }

    #[test]
    fn print_section_exercises_row_rendering() {
        let rows = [
            AboutRow::plain("Service", "api"),
            AboutRow::colored("Port", "80", "80"),
        ];

        print_section("Services", &rows);
        print_row(&rows[0], 140);
    }
}
