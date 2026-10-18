//! Token color mapping for output labels.

use colored::Colorize;

#[derive(Clone, Copy)]
pub(in crate::output) enum TokenColor {
    Green,
    Red,
    Blue,
    Yellow,
    Cyan,
    Orange,
    BrandBlue,
    RedisRed,
    Purple,
    Gray,
    PostgresBlue,
    MysqlBlue,
    MariaBlue,
    ObjectStoreRed,
    RustOrange,
    SearchGreen,
    Teal,
}

pub(in crate::output) fn token_color(token: &str) -> TokenColor {
    match token {
        "out" => TokenColor::Green,
        "err" => TokenColor::Red,
        "api" => TokenColor::Blue,
        "app" | "laravel" | "frankenphp" => TokenColor::Orange,
        "mongodb" | "mongo" => TokenColor::Green,
        "postgres" | "pg" | "database" | "db" => TokenColor::PostgresBlue,
        "mysql" => TokenColor::MysqlBlue,
        "mariadb" => TokenColor::MariaBlue,
        "redis" | "valkey" | "memcached" | "cache" => TokenColor::RedisRed,
        "garage" => TokenColor::ObjectStoreRed,
        "rustfs" | "object-store" | "object_store" => TokenColor::RustOrange,
        "meilisearch" => TokenColor::SearchGreen,
        "opensearch" | "elasticsearch" | "typesense" | "search" => TokenColor::Teal,
        "gotenberg" => TokenColor::BrandBlue,
        "mailhog" | "mailpit" | "dusk" | "selenium" | "rabbitmq" | "soketi" | "scheduler" => {
            TokenColor::Purple
        }
        "caddy" => TokenColor::Yellow,
        "swarm" => TokenColor::Gray,
        _ => TokenColor::Cyan,
    }
}

pub(in crate::output) fn colorize_token_label(token: &str, color: TokenColor) -> String {
    match color {
        TokenColor::Green => token.green().to_string(),
        TokenColor::Red => token.red().to_string(),
        TokenColor::Blue => token.blue().to_string(),
        TokenColor::Yellow => token.yellow().to_string(),
        TokenColor::Cyan => token.cyan().to_string(),
        TokenColor::Orange => token.truecolor(255, 143, 0).to_string(),
        TokenColor::BrandBlue => token.truecolor(52, 120, 246).to_string(),
        TokenColor::RedisRed => token.truecolor(220, 53, 69).to_string(),
        TokenColor::Purple => token.truecolor(146, 103, 196).to_string(),
        TokenColor::Gray => token.dimmed().to_string(),
        TokenColor::PostgresBlue => token.truecolor(51, 103, 145).to_string(),
        TokenColor::MysqlBlue => token.truecolor(0, 117, 143).to_string(),
        TokenColor::MariaBlue => token.truecolor(0, 130, 199).to_string(),
        TokenColor::ObjectStoreRed => token.truecolor(198, 40, 40).to_string(),
        TokenColor::RustOrange => token.truecolor(206, 121, 0).to_string(),
        TokenColor::SearchGreen => token.truecolor(46, 204, 113).to_string(),
        TokenColor::Teal => token.truecolor(0, 162, 255).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{TokenColor, colorize_token_label, token_color};

    #[test]
    fn token_color_matches_known_services() {
        assert!(matches!(token_color("app"), TokenColor::Orange));
        assert!(matches!(token_color("postgres"), TokenColor::PostgresBlue));
        assert!(matches!(token_color("redis"), TokenColor::RedisRed));
        assert!(matches!(token_color("caddy"), TokenColor::Yellow));
        assert!(matches!(token_color("unknown-service"), TokenColor::Cyan));
    }

    #[test]
    fn token_color_groups_aliases_together() {
        assert!(matches!(token_color("mailpit"), TokenColor::Purple));
        assert!(matches!(token_color("soketi"), TokenColor::Purple));
        assert!(matches!(token_color("rustfs"), TokenColor::RustOrange));
        assert!(matches!(
            token_color("object-store"),
            TokenColor::RustOrange
        ));
    }

    #[test]
    fn colorize_token_preserves_token_text() {
        let token = "app";
        let colored = colorize_token_label(token, TokenColor::Orange);
        assert!(colored.contains(token));
    }
}
