use ratatui::style::Color;
use ratatui::widgets::BorderType;

use crate::config::Config;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub related: Color,
    pub status: Color,
    pub error: Color,
    pub code: Color,
    pub code_bg: Color,
    pub image: Color,
    pub math: Color,
    pub diagram: Color,
    pub highlight_fg: Color,
    pub highlight_bg: Color,
    pub scrollbar: Color,
    pub scrollbar_thumb: Color,
    pub border_type: BorderType,
}

impl Theme {
    pub fn from_config(config: &Config) -> Self {
        Self {
            title: parse_color(&config.colors.palette.title, Color::LightYellow),
            text: parse_color(&config.colors.palette.text, Color::Reset),
            muted: parse_color(&config.colors.palette.muted, Color::DarkGray),
            border: parse_color(&config.colors.palette.border, Color::DarkGray),
            accent: parse_color(&config.colors.palette.accent, Color::Cyan),
            related: parse_color(&config.colors.palette.related, Color::Gray),
            status: parse_color(&config.colors.palette.status, Color::Cyan),
            error: parse_color(&config.colors.palette.error, Color::Red),
            code: parse_color(&config.colors.palette.code, Color::Gray),
            code_bg: parse_color(&config.colors.palette.code_bg, Color::Rgb(48, 48, 48)),
            image: parse_color(&config.colors.palette.image, Color::Gray),
            math: parse_color(&config.colors.palette.math, Color::LightYellow),
            diagram: parse_color(&config.colors.palette.diagram, Color::LightMagenta),
            highlight_fg: parse_color(&config.colors.palette.highlight_fg, Color::White),
            highlight_bg: parse_color(&config.colors.palette.highlight_bg, Color::DarkGray),
            scrollbar: parse_color(&config.colors.palette.scrollbar, Color::DarkGray),
            scrollbar_thumb: parse_color(&config.colors.palette.scrollbar_thumb, Color::Cyan),
            border_type: match config.border_style.line.to_ascii_lowercase().as_str() {
                "single" | "plain" => BorderType::Plain,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                "double" => BorderType::Double,
                _ => BorderType::Double,
            },
        }
    }
}

/// Accepts `#RRGGBB`, `RRGGBB`, ANSI names (`red`, `dark-gray`, ...) and
/// indexed colors (`42`). `default`/`reset` map to the terminal default.
fn parse_color(value: &str, fallback: Color) -> Color {
    let value = value.trim();

    if value.is_empty()
        || value.eq_ignore_ascii_case("default")
        || value.eq_ignore_ascii_case("reset")
    {
        return Color::Reset;
    }

    let candidate = if value.len() == 6 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        format!("#{value}")
    } else {
        value.to_string()
    };

    candidate.parse::<Color>().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_named_and_indexed_colors() {
        assert_eq!(parse_color("#FF0000", Color::Reset), Color::Rgb(255, 0, 0));
        assert_eq!(parse_color("00ff00", Color::Reset), Color::Rgb(0, 255, 0));
        assert_eq!(parse_color("red", Color::Reset), Color::Red);
        assert_eq!(parse_color("dark-gray", Color::Reset), Color::DarkGray);
        assert_eq!(parse_color("42", Color::Reset), Color::Indexed(42));
        assert_eq!(parse_color("default", Color::Red), Color::Reset);
        assert_eq!(parse_color("#FFF", Color::Reset), Color::Reset);
        assert_eq!(parse_color("", Color::Reset), Color::Reset);
    }

    #[test]
    fn maps_border_style_from_config() {
        let mut config = Config::default();
        config.border_style.line = "rounded".to_string();
        assert_eq!(Theme::from_config(&config).border_type, BorderType::Rounded);
    }

    #[test]
    fn maps_every_configured_color() {
        let mut config = Config::default();
        config.colors.palette.title = "red".to_string();
        config.colors.palette.border = "#123456".to_string();
        config.colors.palette.scrollbar_thumb = "green".to_string();

        let theme = Theme::from_config(&config);

        assert_eq!(theme.title, Color::Red);
        assert_eq!(theme.border, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.scrollbar_thumb, Color::Green);
    }
}
