use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::{DynamicImage, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui_image::picker::Picker;
use ratatui_image::picker::ProtocolType;
use ratatui_image::protocol::StatefulProtocol;

use crate::config::Images;
use crate::links;

const MAX_DIMENSION: u32 = 16_384;
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

pub struct ImageManager {
    picker: Picker,
    agent: ureq::Agent,
    config: Images,
    cache: HashMap<PathBuf, CachedImage>,
    image_cache: HashMap<PathBuf, Result<DynamicImage, String>>,
    quadrant_cache: HashMap<(PathBuf, u16, u16), RgbaImage>,
    force_quadrants: bool,
}

enum CachedImage {
    Ready(StatefulProtocol),
    Error(String),
}

impl ImageManager {
    pub fn new(config: &Images) -> Self {
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let mut force_quadrants = false;

        match config.protocol.to_ascii_lowercase().as_str() {
            "halfblocks" | "half-blocks" | "blocks" | "quadrants" => {
                picker.set_protocol_type(ProtocolType::Halfblocks);
                force_quadrants = true;
            }
            "kitty" => picker.set_protocol_type(ProtocolType::Kitty),
            "sixel" => picker.set_protocol_type(ProtocolType::Sixel),
            "iterm2" | "iterm" => picker.set_protocol_type(ProtocolType::Iterm2),
            _ => {}
        }

        let mut manager = Self::with_picker(config.clone(), picker);
        manager.force_quadrants = force_quadrants;
        manager
    }

    pub fn uses_quadrants(&self) -> bool {
        self.force_quadrants || matches!(self.picker.protocol_type(), ProtocolType::Halfblocks)
    }

    pub fn quadrants(
        &mut self,
        path: &Path,
        width: u16,
        height: u16,
    ) -> Result<&RgbaImage, String> {
        let key = (path.to_path_buf(), width, height);

        if !self.quadrant_cache.contains_key(&key) {
            let target_width = u32::from(width).saturating_mul(2).max(1);
            let target_height = u32::from(height).saturating_mul(2).max(1);
            let resized = self
                .dynamic(path)?
                .resize_exact(
                    target_width,
                    target_height,
                    image::imageops::FilterType::Lanczos3,
                )
                .to_rgba8();
            self.quadrant_cache.insert(key.clone(), resized);
        }

        match self.quadrant_cache.get(&key) {
            Some(image) => Ok(image),
            None => Err("quadrant image is not available".to_string()),
        }
    }

    fn with_picker(config: Images, picker: Picker) -> Self {
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(DOWNLOAD_TIMEOUT))
                .build(),
        );

        Self {
            picker,
            agent,
            config,
            cache: HashMap::new(),
            image_cache: HashMap::new(),
            quadrant_cache: HashMap::new(),
            force_quadrants: false,
        }
    }

    /// Resolve an image destination relative to the note, enforce the configured
    /// type and size limits, and return an absolute path inside the store.
    /// `http://` and `https://` destinations are downloaded once into
    /// `.cache/images/`.
    pub fn resolve(&self, root: &Path, base_dir: &Path, dest: &str) -> Result<PathBuf, String> {
        if !self.config.enable {
            return Err("images are disabled in config".to_string());
        }

        if dest.starts_with("http://") || dest.starts_with("https://") {
            return self.resolve_remote(root, dest);
        }

        let dest = dest.strip_prefix("file://").unwrap_or(dest);
        let path = links::resolve_path(root, base_dir, dest)
            .ok_or_else(|| "image path is outside the knowledge base or is a URL".to_string())?;

        self.validate_image(&root.join(path))
    }

    fn resolve_remote(&self, root: &Path, url: &str) -> Result<PathBuf, String> {
        let directory = root.join(".cache").join("images");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;

        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        let stem = format!("{:016x}", hasher.finish());

        for extension in ["png", "jpg", "jpeg", "gif", "webp"] {
            let cached = directory.join(format!("{stem}.{extension}"));

            if cached.is_file() {
                return self.validate_image(&cached);
            }
        }

        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", concat!("jff/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|error| format!("cannot download image: {error}"))?;
        let bytes = response
            .body_mut()
            .read_to_vec()
            .map_err(|error| format!("cannot read image response: {error}"))?;

        self.validate_size(bytes.len() as u64)?;

        let format = image::guess_format(&bytes)
            .map_err(|error| format!("unsupported image format: {error}"))?;
        let extension = format
            .extensions_str()
            .first()
            .copied()
            .unwrap_or("img")
            .to_ascii_lowercase();
        self.validate_extension(&extension)?;

        let path = directory.join(format!("{stem}.{extension}"));
        fs::write(&path, &bytes).map_err(|error| error.to_string())?;

        if let Err(error) = self.validate_image(&path) {
            let _ = fs::remove_file(&path);
            return Err(error);
        }

        Ok(path)
    }

    fn validate_image(&self, path: &Path) -> Result<PathBuf, String> {
        if !path.is_file() {
            return Err(format!("image file not found: {}", path.display()));
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        self.validate_extension(&extension)?;

        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        self.validate_size(metadata.len())?;

        let (width, height) = image::image_dimensions(path).map_err(|error| error.to_string())?;
        let pixels = u64::from(width) * u64::from(height);

        if self.config.max_pixels > 0 && pixels > self.config.max_pixels {
            return Err(format!(
                "image has too many pixels ({pixels}, limit {})",
                self.config.max_pixels
            ));
        }

        Ok(path.to_path_buf())
    }

    fn validate_extension(&self, extension: &str) -> Result<(), String> {
        if list_contains(&self.config.exclude, extension) {
            return Err(format!("image type `{extension}` is excluded"));
        }

        if !self.config.mime.is_empty() && !list_contains(&self.config.mime, extension) {
            return Err(format!(
                "image type `{extension}` is not in the allowed MIME list"
            ));
        }

        Ok(())
    }

    fn validate_size(&self, size: u64) -> Result<(), String> {
        if self.config.max_size_kb > 0 {
            let limit = self.config.max_size_kb.saturating_mul(1024);

            if size > limit {
                return Err(format!(
                    "image is too large ({size} bytes, limit {limit} bytes)"
                ));
            }
        }

        Ok(())
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
        self.image_cache.clear();
        self.quadrant_cache.clear();
    }

    fn dynamic(&mut self, path: &Path) -> Result<&DynamicImage, String> {
        if !self.image_cache.contains_key(path) {
            let image = self.decode(path);
            self.image_cache.insert(path.to_path_buf(), image);
        }

        match self.image_cache.get(path) {
            Some(Ok(image)) => Ok(image),
            Some(Err(error)) => Err(error.clone()),
            None => Err("image is not available".to_string()),
        }
    }

    fn load(&self, path: &Path) -> Result<StatefulProtocol, String> {
        Ok(self.picker.new_resize_protocol(self.decode(path)?))
    }

    fn decode(&self, path: &Path) -> Result<DynamicImage, String> {
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

        Ok(image)
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

/// Draw a pre-resized RGBA image using 2x2 Unicode quadrants.
pub(crate) fn draw_quadrants(buffer: &mut Buffer, area: Rect, image: &RgbaImage) {
    let needed_width = u32::from(area.width) * 2;
    let needed_height = u32::from(area.height) * 2;

    if image.width() < needed_width || image.height() < needed_height {
        return;
    }

    for y in 0..area.height {
        for x in 0..area.width {
            let left = u32::from(x) * 2;
            let top = u32::from(y) * 2;
            let pixels = [
                rgba(image.get_pixel(left, top)),
                rgba(image.get_pixel(left + 1, top)),
                rgba(image.get_pixel(left, top + 1)),
                rgba(image.get_pixel(left + 1, top + 1)),
            ];
            let (character, foreground, background) = quadrant_cell(pixels);

            if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(character)
                    .set_fg(foreground)
                    .set_bg(background);
            }
        }
    }
}

fn quadrant_cell(pixels: [[u8; 4]; 4]) -> (char, Color, Color) {
    if pixels.iter().all(|pixel| pixel[3] < 16) {
        return (' ', Color::Reset, Color::Reset);
    }

    let colors = pixels.map(|pixel| [pixel[0], pixel[1], pixel[2]]);
    let mut first = 0usize;
    let mut second = 1usize;
    let mut best_distance = 0u32;

    for left in 0..4 {
        for right in (left + 1)..4 {
            let distance = distance2(colors[left], colors[right]);

            if distance > best_distance {
                best_distance = distance;
                first = left;
                second = right;
            }
        }
    }

    if best_distance == 0 {
        let color = average(&colors, &[true; 4]);
        let color = Color::Rgb(color[0], color[1], color[2]);
        return ('█', color, color);
    }

    let center_first = colors[first];
    let center_second = colors[second];
    let mut assigned = [false; 4];
    let mut mask = 0usize;

    for (index, color) in colors.iter().enumerate() {
        assigned[index] = distance2(*color, center_first) <= distance2(*color, center_second);

        if assigned[index] {
            mask |= 1 << index;
        }
    }

    let foreground = average(&colors, &assigned);
    let background = average(&colors, &assigned.map(|is_foreground| !is_foreground));

    (
        QUADRANTS[mask],
        Color::Rgb(foreground[0], foreground[1], foreground[2]),
        Color::Rgb(background[0], background[1], background[2]),
    )
}

fn rgba(pixel: &image::Rgba<u8>) -> [u8; 4] {
    pixel.0
}

fn distance2(left: [u8; 3], right: [u8; 3]) -> u32 {
    let red = i32::from(left[0]) - i32::from(right[0]);
    let green = i32::from(left[1]) - i32::from(right[1]);
    let blue = i32::from(left[2]) - i32::from(right[2]);

    (red * red + green * green + blue * blue) as u32
}

fn average(colors: &[[u8; 3]; 4], selected: &[bool; 4]) -> [u8; 3] {
    let mut red = 0u32;
    let mut green = 0u32;
    let mut blue = 0u32;
    let mut count = 0u32;

    for (index, color) in colors.iter().enumerate() {
        if selected[index] {
            red += u32::from(color[0]);
            green += u32::from(color[1]);
            blue += u32::from(color[2]);
            count += 1;
        }
    }

    if count == 0 {
        return colors[0];
    }

    [
        (red / count) as u8,
        (green / count) as u8,
        (blue / count) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::{Cursor, Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;
    use tempfile::TempDir;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode png");
        bytes
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        fs::write(path, png_bytes(width, height)).expect("write png");
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
    fn quadrant_cells_map_2x2_patterns() {
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];

        let (character, foreground, background) = quadrant_cell([red, blue, blue, blue]);
        assert_eq!(character, '▘');
        assert_eq!(foreground, Color::Rgb(255, 0, 0));
        assert_eq!(background, Color::Rgb(0, 0, 255));

        let (character, foreground, background) = quadrant_cell([red; 4]);
        assert_eq!(character, '█');
        assert_eq!(foreground, Color::Rgb(255, 0, 0));
        assert_eq!(background, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn downloads_remote_images_to_the_cache() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path();
        let bytes = png_bytes(2, 2);
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let address = listener.local_addr().expect("address");

        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);

            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0u8; 4096];
                        let _ = stream.read(&mut request);
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&bytes);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        let manager = ImageManager::halfblocks(&Images::default());
        let url = format!("http://{address}/picture.png");
        let path = manager
            .resolve(root, root, &url)
            .expect("download remote image");

        server.join().expect("server");
        assert!(path.is_file());
        assert!(path.starts_with(root.join(".cache/images")));

        let cached = manager
            .resolve(root, root, &url)
            .expect("use cached remote image");
        assert_eq!(path, cached);
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
