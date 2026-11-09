use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub fn root() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("JFF_HOME")
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }

    let base = BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(base.home_dir().join(".jff"))
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join("config.json")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor: Editor,
    pub images: Images,
    pub git: Git,
    pub colors: Colors,
    #[serde(rename = "border-style")]
    pub border_style: BorderStyle,
    pub font: Font,
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let path = config_path(root);

        match fs::read_to_string(&path) {
            Ok(contents) => {
                let mut config: Self = serde_json::from_str(&contents)?;
                config.load_theme(root)?;
                Ok(config)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err.into()),
        }
    }

    /// Load the theme referenced by `colors.theme`. A theme completely replaces
    /// the inline palette from `config.json`.
    fn load_theme(&mut self, root: &Path) -> Result<()> {
        let Some(theme) = self.colors.theme.clone() else {
            return Ok(());
        };
        let path = resolve_theme_path(root, &theme);
        let contents = fs::read_to_string(&path).map_err(|error| {
            Error::Theme(format!("cannot read theme `{}`: {error}", path.display()))
        })?;
        let value: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
            Error::Theme(format!(
                "invalid JSON in theme `{}`: {error}",
                path.display()
            ))
        })?;
        let value = value.get("colors").cloned().unwrap_or(value);
        let palette: ThemeColors = serde_json::from_value(value).map_err(|error| {
            Error::Theme(format!("invalid theme `{}`: {error}", path.display()))
        })?;

        self.colors.palette = palette;
        Ok(())
    }

    /// Create the default config only when it does not exist yet.
    pub fn write_default(root: &Path) -> Result<bool> {
        fs::create_dir_all(root)?;
        fs::create_dir_all(root.join("themes"))?;
        let path = config_path(root);

        if path.exists() {
            return Ok(false);
        }

        let contents = serde_json::to_string_pretty(&Self::default())?;
        fs::write(path, format!("{contents}\n"))?;
        Ok(true)
    }
}

fn resolve_theme_path(root: &Path, theme: &Path) -> PathBuf {
    if let Some(value) = theme.to_str()
        && let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = BaseDirs::new()
    {
        return home.home_dir().join(rest);
    }

    if theme.is_absolute() {
        theme.to_path_buf()
    } else {
        root.join(theme)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Editor {
    pub name: String,
    pub path: Option<PathBuf>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            name: "nano".to_string(),
            path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Images {
    pub enable: bool,
    #[serde(default = "default_image_mime", deserialize_with = "string_or_vec")]
    pub mime: Vec<String>,
    #[serde(default = "default_image_exclude", deserialize_with = "string_or_vec")]
    pub exclude: Vec<String>,
    /// Maximum image file size in kilobytes. `0` disables the check.
    #[serde(default = "default_max_image_kb")]
    pub max_size_kb: u64,
    /// Maximum decoded pixel count. `0` disables the check.
    #[serde(default = "default_max_image_pixels")]
    pub max_pixels: u64,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            enable: true,
            mime: default_image_mime(),
            exclude: default_image_exclude(),
            max_size_kb: default_max_image_kb(),
            max_pixels: default_max_image_pixels(),
        }
    }
}

fn default_image_mime() -> Vec<String> {
    ["jpg", "jpeg", "png"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

fn default_image_exclude() -> Vec<String> {
    ["gif", "webp"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

fn default_max_image_kb() -> u64 {
    10_240
}

fn default_max_image_pixels() -> u64 {
    40_000_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Git {
    pub repo: Option<String>,
    pub autopush: bool,
    pub user: Option<String>,
    /// Name of the environment variable that holds the token.
    /// Credentials are never read from the config file.
    pub token_env: String,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            repo: None,
            autopush: false,
            user: None,
            token_env: "JFF_GIT_TOKEN".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    /// Optional path to a theme file. When set, its palette replaces all
    /// inline colors below.
    pub theme: Option<PathBuf>,
    #[serde(flatten)]
    pub palette: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColors {
    pub title: String,
    pub text: String,
    pub muted: String,
    pub border: String,
    pub accent: String,
    pub related: String,
    pub status: String,
    pub error: String,
    pub code: String,
    pub code_bg: String,
    pub image: String,
    pub highlight_fg: String,
    pub highlight_bg: String,
    pub scrollbar: String,
    pub scrollbar_thumb: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            title: "#FFFF00".to_string(),
            text: "#FFFFFF".to_string(),
            muted: "#808080".to_string(),
            border: "#808080".to_string(),
            accent: "#00FFFF".to_string(),
            related: "#C0C0C0".to_string(),
            status: "#00FFFF".to_string(),
            error: "#FF5555".to_string(),
            code: "#C0C0C0".to_string(),
            code_bg: "#303030".to_string(),
            image: "#C0C0C0".to_string(),
            highlight_fg: "#FFFFFF".to_string(),
            highlight_bg: "#444444".to_string(),
            scrollbar: "#444444".to_string(),
            scrollbar_thumb: "#00FFFF".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BorderStyle {
    pub line: String,
    pub border: u16,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            line: "double".to_string(),
            border: 1,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Font {
    pub title: FontStyle,
    pub text: FontStyle,
    pub related: FontStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontStyle {
    pub family: String,
    #[serde(alias = "wight")]
    pub weight: u16,
    pub size: String,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self {
            family: "sans-serif".to_string(),
            weight: 400,
            size: "10pt".to_string(),
        }
    }
}

fn string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    let value = OneOrMany::deserialize(deserializer)?;

    Ok(match value {
        OneOrMany::One(text) => text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        OneOrMany::Many(items) => items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_partial_spec_style_config() {
        let config: Config = serde_json::from_str(
            r#"{
                "editor": { "name": "vim", "path": "/usr/bin/vim" },
                "images": { "mime": "jpg,png", "exclude": "gif" },
                "font": { "title": { "family": "Verdana", "wight": 600, "size": "30px" } }
            }"#,
        )
        .expect("config should parse");

        assert_eq!(config.editor.name, "vim");
        assert_eq!(config.images.mime, ["jpg", "png"]);
        assert_eq!(config.images.exclude, ["gif"]);
        assert_eq!(config.font.title.weight, 600);
        assert_eq!(config.git.token_env, "JFF_GIT_TOKEN");
        assert_eq!(config.images.max_size_kb, 10_240);
        assert_eq!(config.images.max_pixels, 40_000_000);
        assert_eq!(config.colors.palette.border, "#808080");
        assert_eq!(config.colors.palette.error, "#FF5555");
        assert!(config.images.enable);
    }

    #[test]
    fn loads_a_theme_and_ignores_inline_colors() {
        let directory = tempfile::TempDir::new().expect("temp dir");
        let root = directory.path();
        std::fs::create_dir_all(root.join("themes")).expect("themes dir");
        std::fs::write(
            root.join("themes/dracula.json"),
            r##"{"title":"#BD93F9","border":"red"}"##,
        )
        .expect("theme");
        std::fs::write(
            root.join("config.json"),
            r##"{"colors":{"theme":"themes/dracula.json","title":"#FFFFFF","border":"#000000"}}"##,
        )
        .expect("config");

        let config = Config::load(root).expect("load config");

        assert_eq!(
            config.colors.theme.as_deref(),
            Some(std::path::Path::new("themes/dracula.json"))
        );
        assert_eq!(config.colors.palette.title, "#BD93F9");
        assert_eq!(config.colors.palette.border, "red");
        assert_eq!(config.colors.palette.muted, "#808080");
    }

    #[test]
    fn supports_themes_wrapped_in_a_colors_object() {
        let directory = tempfile::TempDir::new().expect("temp dir");
        let root = directory.path();
        std::fs::create_dir_all(root.join("themes")).expect("themes dir");
        std::fs::write(
            root.join("themes/wrapped.json"),
            r#"{"colors":{"title":"green","text":"white"}}"#,
        )
        .expect("theme");
        std::fs::write(
            root.join("config.json"),
            r#"{"colors":{"theme":"themes/wrapped.json"}}"#,
        )
        .expect("config");

        let config = Config::load(root).expect("load config");

        assert_eq!(config.colors.palette.title, "green");
        assert_eq!(config.colors.palette.text, "white");
    }

    #[test]
    fn reports_a_missing_theme() {
        let directory = tempfile::TempDir::new().expect("temp dir");
        let root = directory.path();
        std::fs::write(
            root.join("config.json"),
            r#"{"colors":{"theme":"themes/missing.json"}}"#,
        )
        .expect("config");

        let error = Config::load(root).expect_err("missing theme must fail");

        assert!(matches!(error, Error::Theme(_)));
        assert!(error.to_string().contains("cannot read theme"));
    }

    #[test]
    fn config_init_creates_the_themes_directory() {
        let directory = tempfile::TempDir::new().expect("temp dir");

        Config::write_default(directory.path()).expect("write config");

        assert!(directory.path().join("themes").is_dir());
        assert!(directory.path().join("config.json").is_file());
    }

    #[test]
    fn accepts_arrays_and_strings() {
        let config: Config =
            serde_json::from_str(r#"{"images": {"mime": ["png", "jpg"], "exclude": "gif,webp"}}"#)
                .expect("config should parse");

        assert_eq!(config.images.mime, ["png", "jpg"]);
        assert_eq!(config.images.exclude, ["gif", "webp"]);
    }
}
