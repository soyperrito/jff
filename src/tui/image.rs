use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::config::Images;
use crate::links;

const MAX_DIMENSION: u32 = 16_384;
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

pub struct ImageManager {
    picker: Picker,
    config: Images,
    cache: HashMap<PathBuf, CachedImage>,
}

enum CachedImage {
    Ready(StatefulProtocol),
    Error(String),
}

impl ImageManager {
    pub fn new(config: &Images) -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        Self::with_picker(config.clone(), picker)
    }

    fn with_picker(config: Images, picker: Picker) -> Self {
        Self {
            picker,
            config,
            cache: HashMap::new(),
        }
    }

    /// Resolve an image destination relative to the note, enforce the configured
    /// type and size limits, and return an absolute path inside the store.
    pub fn resolve(&self, root: &Path, base_dir: &Path, dest: &str) -> Result<PathBuf, String> {
        if !self.config.enable {
            return Err("images are disabled in config".to_string());
        }

        let dest = dest.strip_prefix("file://").unwrap_or(dest);
        let path = links::resolve_path(root, base_dir, dest)
            .ok_or_else(|| "image path is outside the knowledge base or is a URL".to_string())?;

        let path = root.join(path);

        if !path.is_file() {
            return Err(format!("image file not found: {}", path.display()));
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if list_contains(&self.config.exclude, &extension) {
            return Err(format!("image type `{extension}` is excluded"));
        }

        if !self.config.mime.is_empty() && !list_contains(&self.config.mime, &extension) {
            return Err(format!(
                "image type `{extension}` is not in the allowed MIME list"
            ));
        }

        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;

        if self.config.max_size_kb > 0 {
            let limit = self.config.max_size_kb.saturating_mul(1024);

            if metadata.len() > limit {
                return Err(format!(
                    "image is too large ({} bytes, limit {limit} bytes)",
                    metadata.len()
                ));
            }
        }

        let (width, height) = image::image_dimensions(&path).map_err(|error| error.to_string())?;
        let pixels = u64::from(width) * u64::from(height);

        if self.config.max_pixels > 0 && pixels > self.config.max_pixels {
            return Err(format!(
                "image has too many pixels ({pixels}, limit {})",
                self.config.max_pixels
            ));
        }

        Ok(path)
    }

    pub fn protocol(&mut self, path: &Path) -> Result<&mut StatefulProtocol, String> {
        if !self.cache.contains_key(path) {
            let cached = match self.load(path) {
                Ok(protocol) => CachedImage::Ready(protocol),
                Err(error) => CachedImage::Error(error),
            };
            self.cache.insert(path.to_path_buf(), cached);
        }

        match self.cache.get_mut(path).expect("cache entry exists") {
            CachedImage::Ready(protocol) => Ok(protocol),
            CachedImage::Error(error) => Err(error.clone()),
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    fn load(&self, path: &Path) -> Result<StatefulProtocol, String> {
        let mut reader = image::ImageReader::open(path)
            .map_err(|error| error.to_string())?
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(MAX_DECODE_ALLOC);
        reader.limits(limits);

        let image = reader.decode().map_err(|error| error.to_string())?;
        let pixels = u64::from(image.width()) * u64::from(image.height());

        if self.config.max_pixels > 0 && pixels > self.config.max_pixels {
            return Err(format!(
                "image has too many pixels ({pixels}, limit {})",
                self.config.max_pixels
            ));
        }

        Ok(self.picker.new_resize_protocol(image))
    }

    #[cfg(test)]
    fn halfblocks(config: &Images) -> Self {
        Self::with_picker(config.clone(), Picker::halfblocks())
    }
}

fn list_contains(list: &[String], extension: &str) -> bool {
    list.iter().any(|item| {
        let item = item.trim().trim_start_matches('.');
        let item = item.rsplit('/').next().unwrap_or(item);
        item.eq_ignore_ascii_case(extension)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use tempfile::TempDir;

    fn write_png(path: &Path, width: u32, height: u32) {
        let image = RgbaImage::from_pixel(width, height, Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode png");
        fs::write(path, bytes).expect("write png");
    }

    #[test]
    fn rejects_excluded_extensions() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        let path = root.join("bad.gif");
        fs::write(&path, b"not an image").expect("file");

        let manager = ImageManager::halfblocks(&Images::default());
        let error = manager
            .resolve(root, root, "bad.gif")
            .expect_err("gif is excluded");

        assert!(error.contains("excluded"));
    }

    #[test]
    fn rejects_paths_outside_the_store() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        let manager = ImageManager::halfblocks(&Images::default());

        let error = manager
            .resolve(root, root, "../../etc/passwd")
            .expect_err("path must be rejected");

        assert!(error.contains("outside"));
    }

    #[test]
    fn enforces_file_size_limits() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        let path = root.join("large.png");
        fs::write(&path, vec![0u8; 2048]).expect("file");

        let config = Images {
            max_size_kb: 1,
            ..Images::default()
        };
        let manager = ImageManager::halfblocks(&config);
        let error = manager
            .resolve(root, root, "large.png")
            .expect_err("file is too large");

        assert!(error.contains("too large"));
    }

    #[test]
    fn enforces_pixel_limits() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        write_png(&root.join("photo.png"), 4, 4);

        let config = Images {
            max_pixels: 8,
            ..Images::default()
        };
        let manager = ImageManager::halfblocks(&config);
        let error = manager
            .resolve(root, root, "photo.png")
            .expect_err("pixel limit exceeded");

        assert!(error.contains("too many pixels"));
    }

    #[test]
    fn loads_an_image_with_halfblocks() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        let path = root.join("photo.png");
        write_png(&path, 2, 2);

        let manager = ImageManager::halfblocks(&Images::default());
        let resolved = manager
            .resolve(root, root, "photo.png")
            .expect("resolve image");

        let mut manager = manager;
        assert!(manager.protocol(&resolved).is_ok());
    }
}
