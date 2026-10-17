//! output colorize module.
//!
//! Contains output colorization helpers used by Berth command workflows.

const TOKEN_COLUMN_WIDTHS: [usize; 2] = [16, 16];
mod payload_scan;
mod payload_style;
mod token;
use payload_scan::{
    is_image_like_token, is_token_start, scan_until_whitespace, summary_count_span,
};
use payload_style::{
    style_image_like, style_number, style_path, style_quoted, style_summary_count, style_url,
};
#[cfg(test)]
pub(super) use token::TokenColor;
pub(super) use token::{colorize_token_label, token_color};

pub(super) fn colorize_message_tokens(message: &str) -> String {
    let mut rendered = String::new();
    let mut remaining = message;
    let mut token_index = 0_usize;

    while let Some(after_open) = remaining.strip_prefix('[') {
        let Some((token, tail)) = after_open.split_once(']') else {
            break;
        };

        let token_text = format!("[{token}]");
        let styled = colorize_token_label(&token_text, token_color(token));
        rendered.push_str(&styled);
        if let Some(width) = TOKEN_COLUMN_WIDTHS.get(token_index) {
            let pad = width.saturating_sub(token_text.len()).max(1);
            if pad > 0 {
                rendered.push_str(&" ".repeat(pad));
            }
        }
        token_index += 1;

        remaining = tail.trim_start();
        if !remaining.is_empty() {
            continue;
        }

        break;
    }

    if rendered.is_empty() {
        return colorize_message_payload(message);
    }

    while token_index < TOKEN_COLUMN_WIDTHS.len() {
        rendered.push_str(&" ".repeat(TOKEN_COLUMN_WIDTHS[token_index]));
        token_index += 1;
    }

    if !remaining.is_empty() && !rendered.ends_with(' ') && !remaining.starts_with(' ') {
        rendered.push(' ');
    }
    rendered.push_str(&colorize_message_payload(remaining));
    rendered
}

pub(super) fn colorize_message_payload(message: &str) -> String {
    let mut rendered = String::new();
    let mut index = 0_usize;
    let bytes = message.as_bytes();

    while index < bytes.len() {
        if message[index..].starts_with("https://") || message[index..].starts_with("http://") {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            rendered.push_str(&style_url(token));
            index = end;
            continue;
        }

        if bytes[index] == b'/'
            && (index == 0
                || bytes[index - 1].is_ascii_whitespace()
                || matches!(bytes[index - 1], b'\'' | b'"' | b'('))
        {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            rendered.push_str(&style_path(token));
            index = end;
            continue;
        }

        if bytes[index] == b'\'' {
            let start = index;
            index += 1;
            while index < bytes.len() && bytes[index] != b'\'' {
                index += 1;
            }
            if index < bytes.len() {
                index += 1;
            }
            let token = &message[start..index];
            rendered.push_str(&style_quoted(token));
            continue;
        }

        if bytes[index] == b'`' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'`' {
                end += 1;
            }

            if end < bytes.len() {
                let token = &message[start..end];
                rendered.push_str(&style_quoted(token));
                index = end + 1;
                continue;
            }
        }

        if let Some((end, tone)) = summary_count_span(message, index) {
            let token = &message[index..end];
            rendered.push_str(&style_summary_count(token, tone));
            index = end;
            continue;
        }

        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let end = index;
            let len = end - start;
            let prev_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let next_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
            let token = &message[start..end];
            if prev_ok && next_ok && (2..=5).contains(&len) {
                rendered.push_str(&style_number(token));
            } else {
                rendered.push_str(token);
            }
            continue;
        }

        if is_token_start(bytes, index) {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            if is_image_like_token(token) || token.starts_with("acme-") {
                rendered.push_str(&style_image_like(token));
                index = end;
                continue;
            }
        }

        let ch = message[index..].chars().next().unwrap_or_default();
        rendered.push(ch);
        index += ch.len_utf8();
    }

    rendered
}
