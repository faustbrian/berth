//! Event-level YAML validation and a text-preserving document tree.
//!
//! serde-saphyr resolves plain scalars with YAML 1.1 rules (`off` is a
//! boolean, `0022` a number), while helm and stackctl files were written for
//! serde_yaml's YAML 1.2 core rules. Walking parser events lets berth keep
//! every scalar's exact text and apply the 1.2 rules itself, and enforces the
//! structural limits from the 8.x hostile-YAML work (FI-18) before any value
//! is deserialized.

use anyhow::Result;
use serde_json::{Map, Number, Value};
use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle};

/// Deepest collection nesting accepted in a berth YAML document.
pub(crate) const MAX_DEPTH: usize = 32;
/// Most scalar and collection nodes accepted in one document.
pub(crate) const MAX_NODES: usize = 10_000;

enum Frame {
    Sequence(Vec<Value>),
    Mapping(Map<String, Value>, Option<String>),
}

/// Parses one YAML document into a JSON value tree.
///
/// Quoted and block scalars are always strings. Plain scalars become null
/// (`~`, `null`, empty), booleans (`true`/`false` in their three YAML 1.2
/// spellings) or integers (no leading zeros); every other plain scalar stays
/// a string with its exact text.
///
/// # Errors
///
/// Rejects syntax errors, anchors, aliases, tags, merge keys (`<<`),
/// duplicate or non-scalar mapping keys, more than one document, and
/// documents deeper than [`MAX_DEPTH`] or larger than [`MAX_NODES`].
pub(crate) fn parse_text_tree(content: &str) -> Result<Value> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Value> = None;
    let mut documents = 0_usize;
    let mut nodes = 0_usize;

    for item in Parser::new_from_str(content) {
        let (event, span) = item.map_err(|error| anyhow::anyhow!("invalid YAML: {error}"))?;
        let line = span.start.line();
        let value = match event {
            Event::StreamStart | Event::StreamEnd | Event::DocumentEnd => continue,
            Event::DocumentStart(..) => {
                documents += 1;
                if documents > 1 {
                    anyhow::bail!(
                        "line {line}: berth files must contain exactly one YAML document"
                    );
                }
                continue;
            }
            Event::Alias(_) => {
                anyhow::bail!(
                    "line {line}: YAML aliases are not supported; write the value out in full"
                )
            }
            Event::Scalar(text, style, anchor, tag) => {
                reject_anchor_and_tag(line, anchor, tag.is_some())?;
                count_node(&mut nodes, line)?;
                // Mapping keys keep their exact text, as serde_yaml did: an env
                // key named `TRUE` or `1` is a key, not a boolean or a number.
                if matches!(stack.last(), Some(Frame::Mapping(_, None))) {
                    Value::String(text.into_owned())
                } else {
                    scalar_value(&text, style)
                }
            }
            Event::SequenceStart(_, anchor, tag) => {
                reject_anchor_and_tag(line, anchor, tag.is_some())?;
                count_node(&mut nodes, line)?;
                push_frame(&mut stack, Frame::Sequence(Vec::new()), line)?;
                continue;
            }
            Event::MappingStart(_, anchor, tag) => {
                reject_anchor_and_tag(line, anchor, tag.is_some())?;
                count_node(&mut nodes, line)?;
                push_frame(&mut stack, Frame::Mapping(Map::new(), None), line)?;
                continue;
            }
            Event::SequenceEnd => match stack.pop() {
                Some(Frame::Sequence(items)) => Value::Array(items),
                _ => anyhow::bail!("line {line}: unbalanced YAML sequence"),
            },
            Event::MappingEnd => match stack.pop() {
                Some(Frame::Mapping(map, None)) => Value::Object(map),
                _ => anyhow::bail!("line {line}: unbalanced YAML mapping"),
            },
            #[allow(
                unreachable_patterns,
                reason = "comment events exist only with the comments feature"
            )]
            _ => continue,
        };
        attach(&mut stack, &mut root, value, line)?;
    }

    root.ok_or_else(|| anyhow::anyhow!("the YAML document is empty"))
}

fn reject_anchor_and_tag(line: usize, anchor: usize, tagged: bool) -> Result<()> {
    if anchor != 0 {
        anyhow::bail!("line {line}: YAML anchors are not supported; write the value out in full");
    }
    if tagged {
        anyhow::bail!("line {line}: YAML tags are not supported in berth files");
    }
    Ok(())
}

fn count_node(nodes: &mut usize, line: usize) -> Result<()> {
    *nodes += 1;
    if *nodes > MAX_NODES {
        anyhow::bail!("line {line}: the YAML document has more than {MAX_NODES} nodes");
    }
    Ok(())
}

fn push_frame(stack: &mut Vec<Frame>, frame: Frame, line: usize) -> Result<()> {
    if stack.len() >= MAX_DEPTH {
        anyhow::bail!("line {line}: YAML nesting deeper than {MAX_DEPTH} levels");
    }
    stack.push(frame);
    Ok(())
}

fn attach(stack: &mut [Frame], root: &mut Option<Value>, value: Value, line: usize) -> Result<()> {
    match stack.last_mut() {
        None => {
            *root = Some(value);
            Ok(())
        }
        Some(Frame::Sequence(items)) => {
            items.push(value);
            Ok(())
        }
        Some(Frame::Mapping(map, pending_key)) => match pending_key.take() {
            None => {
                let Value::String(key) = value else {
                    anyhow::bail!("line {line}: mapping keys must be strings");
                };
                if key == "<<" {
                    anyhow::bail!("line {line}: YAML merge keys (`<<`) are not supported");
                }
                if map.contains_key(&key) {
                    anyhow::bail!("line {line}: duplicate key '{key}'");
                }
                *pending_key = Some(key);
                Ok(())
            }
            Some(key) => {
                map.insert(key, value);
                Ok(())
            }
        },
    }
}

fn scalar_value(text: &str, style: ScalarStyle) -> Value {
    if style != ScalarStyle::Plain {
        return Value::String(text.to_owned());
    }
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => plain_integer(text).map_or_else(|| Value::String(text.to_owned()), Value::Number),
    }
}

/// Parses YAML 1.2 core decimal integers; `0022` and `+5` stay strings.
fn plain_integer(text: &str) -> Option<Number> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    if !canonical {
        return None;
    }
    text.parse::<i64>()
        .ok()
        .map(Number::from)
        .or_else(|| text.parse::<u64>().ok().map(Number::from))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MAX_DEPTH, MAX_NODES, parse_text_tree};

    #[test]
    fn keeps_yaml_1_1_words_and_leading_zero_numbers_as_text() -> anyhow::Result<()> {
        let value = parse_text_tree(
            "env:\n  XDEBUG_MODE: off\n  APP_LOCALE: no\n  UMASK: 0022\n  DEBUG: \"true\"\nrestart: no\nport: 3306\nlocalhost_tls: true\nempty: ~\n",
        )?;
        assert_eq!(
            value,
            json!({
                "env": {"XDEBUG_MODE": "off", "APP_LOCALE": "no", "UMASK": "0022", "DEBUG": "true"},
                "restart": "no",
                "port": 3306,
                "localhost_tls": true,
                "empty": null
            })
        );
        Ok(())
    }

    #[test]
    fn rejects_anchors_aliases_tags_merge_keys_and_duplicates() {
        for (document, reason) in [
            ("a: &x 1\n", "anchors"),
            ("a: &x 1\nb: *x\n", "anchors"),
            // An alias of an undefined anchor is already a syntax error.
            ("a: 1\nb: *x\n", "invalid YAML"),
            ("a: !custom x\n", "tags"),
            ("a:\n  <<: {b: 1}\n", "merge keys"),
            ("a: 1\na: 2\n", "duplicate key 'a'"),
            ("a: 1\n---\nb: 2\n", "exactly one YAML document"),
        ] {
            let error = parse_text_tree(document).expect_err(document);
            assert!(
                format!("{error:#}").contains(reason),
                "{document}: {error:#}"
            );
        }
    }

    #[test]
    fn keys_keep_their_text_while_values_are_typed() -> anyhow::Result<()> {
        let value = parse_text_tree("TRUE: true\n1: 1\n")?;
        assert_eq!(value, json!({"TRUE": true, "1": 1}));
        Ok(())
    }

    #[test]
    fn enforces_depth_and_node_budgets() {
        let deep = format!(
            "{}x\n{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        let error = parse_text_tree(&deep).expect_err("too deep");
        assert!(format!("{error:#}").contains("nesting"), "{error:#}");

        let ok_depth = format!("{}x{}\n", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(
            parse_text_tree(&ok_depth).is_ok(),
            "exactly the maximum depth is accepted"
        );

        let wide = format!("[{}]\n", vec!["1"; MAX_NODES].join(","));
        let error = parse_text_tree(&wide).expect_err("too many nodes");
        assert!(format!("{error:#}").contains("nodes"), "{error:#}");
    }
}
