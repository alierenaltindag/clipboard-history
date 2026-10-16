use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use qrcode::render::svg;
use qrcode::QrCode;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;

static COLOR_HEX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").unwrap());

static COLOR_RGB_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^rgba?\s*\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})(?:\s*,\s*([0-9.]+))?\s*\)$",
    )
    .unwrap()
});

static HTML_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct ColorInfo {
    pub hex: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

pub struct TextTransforms;

impl TextTransforms {
    /// Convert string to UPPERCASE.
    pub fn to_uppercase(s: &str) -> String {
        s.to_uppercase()
    }

    /// Convert string to lowercase.
    pub fn to_lowercase(s: &str) -> String {
        s.to_lowercase()
    }

    /// Convert words to Title Case.
    pub fn to_title_case(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut capitalize_next = true;
        for c in s.chars() {
            if c.is_whitespace() || c == '_' || c == '-' {
                capitalize_next = true;
                result.push(c);
            } else if capitalize_next {
                for upper in c.to_uppercase() {
                    result.push(upper);
                }
                capitalize_next = false;
            } else {
                for lower in c.to_lowercase() {
                    result.push(lower);
                }
            }
        }
        result
    }

    /// Convert string to snake_case.
    pub fn to_snake_case(s: &str) -> String {
        let mut result = String::with_capacity(s.len() + 4);
        let mut prev_is_sep = true;

        for c in s.chars() {
            if c.is_alphanumeric() {
                if c.is_uppercase() && !prev_is_sep && !result.is_empty() {
                    result.push('_');
                }
                for lower in c.to_lowercase() {
                    result.push(lower);
                }
                prev_is_sep = false;
            } else if !prev_is_sep {
                result.push('_');
                prev_is_sep = true;
            }
        }

        result.trim_matches('_').to_string()
    }

    /// Convert string to kebab-case.
    pub fn to_kebab_case(s: &str) -> String {
        Self::to_snake_case(s).replace('_', "-")
    }

    /// Convert string to camelCase.
    pub fn to_camel_case(s: &str) -> String {
        let snake = Self::to_snake_case(s);
        let mut result = String::with_capacity(snake.len());
        let mut capitalize_next = false;

        for c in snake.chars() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                for upper in c.to_uppercase() {
                    result.push(upper);
                }
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Formats and indents JSON with 2 spaces.
    pub fn json_prettify(s: &str) -> Result<String, serde_json::Error> {
        let parsed: Value = serde_json::from_str(s)?;
        serde_json::to_string_pretty(&parsed)
    }

    /// Compresses JSON into single-line minified representation.
    pub fn json_minify(s: &str) -> Result<String, serde_json::Error> {
        let parsed: Value = serde_json::from_str(s)?;
        serde_json::to_string(&parsed)
    }

    /// Encodes UTF-8 text to standard Base64.
    pub fn base64_encode(s: &str) -> String {
        BASE64_STD.encode(s.as_bytes())
    }

    /// Decodes Base64 string back to UTF-8 text.
    pub fn base64_decode(s: &str) -> Result<String, String> {
        let bytes = BASE64_STD
            .decode(s.trim())
            .map_err(|e| format!("Base64 decode error: {}", e))?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {}", e))
    }

    /// URL encodes text.
    pub fn url_encode(s: &str) -> String {
        urlencoding::encode(s).into_owned()
    }

    /// URL decodes text.
    pub fn url_decode(s: &str) -> Result<String, String> {
        urlencoding::decode(s)
            .map(|cow| cow.into_owned())
            .map_err(|e| format!("URL decode error: {}", e))
    }

    /// Strips HTML tags and unescapes common entities for clean plain-text paste.
    pub fn strip_formatting(s: &str) -> String {
        let stripped = HTML_TAG_REGEX.replace_all(s, "");
        stripped
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("\r\n", "\n")
    }

    /// Checks if a string represents a CSS Hex or RGB/RGBA color.
    pub fn detect_color(s: &str) -> Option<ColorInfo> {
        let trimmed = s.trim();

        if COLOR_HEX_REGEX.is_match(trimmed) {
            let hex_str = trimmed.trim_start_matches('#');
            match hex_str.len() {
                3 => {
                    let r = u8::from_str_radix(&hex_str[0..1].repeat(2), 16).ok()?;
                    let g = u8::from_str_radix(&hex_str[1..2].repeat(2), 16).ok()?;
                    let b = u8::from_str_radix(&hex_str[2..3].repeat(2), 16).ok()?;
                    Some(ColorInfo {
                        hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
                        r,
                        g,
                        b,
                        a: 1.0,
                    })
                }
                6 => {
                    let r = u8::from_str_radix(&hex_str[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex_str[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex_str[4..6], 16).ok()?;
                    Some(ColorInfo {
                        hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
                        r,
                        g,
                        b,
                        a: 1.0,
                    })
                }
                8 => {
                    let r = u8::from_str_radix(&hex_str[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex_str[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex_str[4..6], 16).ok()?;
                    let a_byte = u8::from_str_radix(&hex_str[6..8], 16).ok()?;
                    Some(ColorInfo {
                        hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
                        r,
                        g,
                        b,
                        a: (a_byte as f32) / 255.0,
                    })
                }
                _ => None,
            }
        } else if let Some(caps) = COLOR_RGB_REGEX.captures(trimmed) {
            let r: u8 = caps.get(1)?.as_str().parse().ok()?;
            let g: u8 = caps.get(2)?.as_str().parse().ok()?;
            let b: u8 = caps.get(3)?.as_str().parse().ok()?;
            let a: f32 = caps
                .get(4)
                .map(|m| m.as_str().parse::<f32>().unwrap_or(1.0))
                .unwrap_or(1.0);
            Some(ColorInfo {
                hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
                r,
                g,
                b,
                a,
            })
        } else {
            None
        }
    }

    /// Generates a standalone SVG string of a QR code representing the text.
    pub fn generate_qr_svg(text: &str) -> Result<String, String> {
        let code =
            QrCode::new(text.as_bytes()).map_err(|e| format!("QR generation error: {}", e))?;
        let svg_image = code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build();
        Ok(svg_image)
    }

    /// Concatenates multiple text items using the specified delimiter format.
    pub fn concatenate(items: &[&str], delimiter: &ConcatDelimiter) -> String {
        match delimiter {
            ConcatDelimiter::NumberedList => items
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
            ConcatDelimiter::BulletList => items
                .iter()
                .map(|s| format!("- {}", s.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => items.join(delimiter.as_str()),
        }
    }
}

impl ColorInfo {
    pub fn to_hex_string(&self) -> String {
        if (self.a - 1.0).abs() < 0.001 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            let alpha_u8 = (self.a * 255.0).round() as u8;
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                self.r, self.g, self.b, alpha_u8
            )
        }
    }

    pub fn to_rgb_string(&self) -> String {
        if (self.a - 1.0).abs() < 0.001 {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!("rgba({}, {}, {}, {:.2})", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_hsl_string(&self) -> String {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let l = (max + min) / 2.0;
        let (h, s) = if delta.abs() < 0.0001 {
            (0.0, 0.0)
        } else {
            let s = if l > 0.5 {
                delta / (2.0 - max - min)
            } else {
                delta / (max + min)
            };
            let h = if (max - r).abs() < 0.0001 {
                ((g - b) / delta + (if g < b { 6.0 } else { 0.0 })) / 6.0
            } else if (max - g).abs() < 0.0001 {
                ((b - r) / delta + 2.0) / 6.0
            } else {
                ((r - g) / delta + 4.0) / 6.0
            };
            (h * 360.0, s * 100.0)
        };

        let l_pct = l * 100.0;
        if (self.a - 1.0).abs() < 0.001 {
            format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s, l_pct)
        } else {
            format!("hsla({:.0}, {:.0}%, {:.0}%, {:.2})", h, s, l_pct, self.a)
        }
    }

    pub fn to_css_var(&self, var_name: &str) -> String {
        format!("--{}: {};", var_name, self.hex)
    }

    pub fn to_glsl_vec4(&self) -> String {
        format!(
            "vec4({:.3}, {:.3}, {:.3}, {:.3})",
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a
        )
    }

    pub fn to_swift_ui(&self) -> String {
        format!(
            "Color(red: {:.3}, green: {:.3}, blue: {:.3}, opacity: {:.3})",
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a
        )
    }
}

pub struct SnippetExpander;

impl SnippetExpander {
    pub fn expand(template: &str, current_clipboard: Option<&str>) -> String {
        let now = chrono::Local::now();
        let mut result = template.to_string();

        result = result.replace("{date}", &now.format("%Y-%m-%d").to_string());
        result = result.replace("{time}", &now.format("%H:%M:%S").to_string());
        result = result.replace("{datetime}", &now.to_rfc3339());
        result = result.replace("{year}", &now.format("%Y").to_string());
        result = result.replace("{month}", &now.format("%m").to_string());
        result = result.replace("{day}", &now.format("%d").to_string());

        while result.contains("{uuid}") {
            result = result.replacen("{uuid}", &uuid::Uuid::new_v4().to_string(), 1);
        }

        if let Some(clip) = current_clipboard {
            result = result.replace("{clipboard}", clip);
        } else {
            result = result.replace("{clipboard}", "");
        }

        result
    }
}

pub struct OcrEngine;

impl OcrEngine {
    /// Check if tesseract binary is installed and executable in PATH
    pub async fn is_available() -> bool {
        tokio::process::Command::new("tesseract")
            .arg("--version")
            .output()
            .await
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    /// Checks if a custom high-accuracy tessdata directory exists (e.g. ~/.local/share/clipboard-history/tessdata)
    pub fn get_custom_tessdata_dir() -> Option<std::path::PathBuf> {
        if let Ok(dir) = std::env::var("CLIPBOARD_HISTORY_TESSDATA") {
            let p = std::path::PathBuf::from(dir);
            if p.is_dir() {
                return Some(p);
            }
        }

        let p = crate::config::AppConfig::data_dir().join("tessdata");
        if p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&p) {
                let has_traineddata = entries.filter_map(|e| e.ok()).any(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "traineddata")
                        .unwrap_or(false)
                });
                if has_traineddata {
                    return Some(p);
                }
            }
        }

        None
    }

    /// Queries installed languages from `tesseract --list-langs` (merging custom tessdata and system models)
    pub async fn get_installed_languages() -> Vec<String> {
        let mut langs = std::collections::BTreeSet::new();

        // 1. Check custom tessdata directory if present
        if let Some(custom_dir) = Self::get_custom_tessdata_dir() {
            let output = tokio::process::Command::new("tesseract")
                .arg("--tessdata-dir")
                .arg(&custom_dir)
                .arg("--list-langs")
                .output()
                .await;

            if let Ok(out) = output {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    for line in stdout.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty()
                            && !trimmed.starts_with("List of available languages")
                        {
                            langs.insert(trimmed.to_string());
                        }
                    }
                }
            }
        }

        // 2. Query system tesseract languages
        let output = tokio::process::Command::new("tesseract")
            .arg("--list-langs")
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with("List of available languages") {
                        langs.insert(trimmed.to_string());
                    }
                }
            }
        }

        langs.into_iter().collect()
    }

    /// Selects an optimal language string (e.g. "tur+eng" or "eng") based on system locale and installed languages.
    /// Note: `osd` is strictly excluded from recognition models as it degrades LSTM accuracy.
    pub fn determine_ocr_languages(installed: &[String]) -> Option<String> {
        if installed.is_empty() {
            return None;
        }

        let has_tur = installed.iter().any(|l| l == "tur");
        let has_eng = installed.iter().any(|l| l == "eng");

        // If Turkish language data is installed, prioritize Turkish + English
        if has_tur {
            return if has_eng {
                Some("tur+eng".to_string())
            } else {
                Some("tur".to_string())
            };
        }

        let locale_lang = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_lowercase();

        let prefix = locale_lang.split(['_', '.', '@', '-']).next().unwrap_or("");
        let mapped_tess_lang = match prefix {
            "tr" => Some("tur"),
            "de" => Some("deu"),
            "fr" => Some("fra"),
            "es" => Some("spa"),
            "it" => Some("ita"),
            "pt" => Some("por"),
            "ru" => Some("rus"),
            "pl" => Some("pol"),
            "nl" => Some("nld"),
            "uk" => Some("ukr"),
            "ar" => Some("ara"),
            "zh" => Some("chi_sim"),
            "ja" => Some("jpn"),
            "ko" => Some("kor"),
            "en" => Some("eng"),
            _ => None,
        };

        if let Some(target_lang) = mapped_tess_lang {
            if installed.iter().any(|l| l == target_lang) {
                if target_lang == "eng" {
                    return Some("eng".to_string());
                } else if has_eng {
                    return Some(format!("{}+eng", target_lang));
                } else {
                    return Some(target_lang.to_string());
                }
            }
        }

        // Fallback if detected locale language is not installed
        if has_eng {
            Some("eng".to_string())
        } else {
            installed
                .iter()
                .find(|l| l.as_str() != "osd")
                .cloned()
                .or_else(|| installed.first().cloned())
        }
    }

    /// Preprocesses image bytes to optimize OCR recognition:
    /// - Composites alpha transparency onto solid white and converts to ITU-R 601 grayscale
    ///   (eliminates LCD subpixel color fringing that corrupts diacritics like dots on i/ö/ü, breves on ğ, cedillas on ç/ş)
    /// - Detects dark mode backgrounds and inverts to dark text on white
    /// - Applies dynamic contrast stretching to maximize separation of text strokes
    /// - Adaptively upscales low-resolution screenshots using Lanczos3
    /// - Adds a 20px solid white border (padding) for reliable page segmentation
    pub fn preprocess_image_bytes(image_bytes: &[u8]) -> Vec<u8> {
        let img = match image::load_from_memory(image_bytes) {
            Ok(i) => i,
            Err(_) => return image_bytes.to_vec(),
        };

        let (orig_w, orig_h) = (img.width(), img.height());
        if orig_w == 0 || orig_h == 0 {
            return image_bytes.to_vec();
        }

        // 1. Composite onto white and convert to grayscale
        let rgba = img.to_rgba8();
        let mut gray = image::GrayImage::new(orig_w, orig_h);
        let mut total_luma: u64 = 0;
        let mut min_luma: u8 = 255;
        let mut max_luma: u8 = 0;

        for (x, y, pixel) in rgba.enumerate_pixels() {
            let a = pixel[3] as f32 / 255.0;
            let r = (pixel[0] as f32 * a) + (255.0 * (1.0 - a));
            let g = (pixel[1] as f32 * a) + (255.0 * (1.0 - a));
            let b = (pixel[2] as f32 * a) + (255.0 * (1.0 - a));

            let luma = (0.299 * r + 0.587 * g + 0.114 * b)
                .round()
                .clamp(0.0, 255.0) as u8;
            total_luma += luma as u64;

            if luma < min_luma {
                min_luma = luma;
            }
            if luma > max_luma {
                max_luma = luma;
            }

            gray.put_pixel(x, y, image::Luma([luma]));
        }

        let pixel_count = (orig_w as u64) * (orig_h as u64);
        let avg_luma = total_luma
            .checked_div(pixel_count)
            .map(|v| v as u8)
            .unwrap_or(255);

        // 2. Dark mode inversion: If background is dark (average luminance < 120),
        // invert colors so light text becomes dark text on a white background.
        if avg_luma < 120 {
            for pixel in gray.pixels_mut() {
                pixel[0] = 255 - pixel[0];
            }
            let old_min = min_luma;
            min_luma = 255 - max_luma;
            max_luma = 255 - old_min;
        }

        // 3. Dynamic contrast stretch:
        // Spreads out pixel luminance so faint diacritics and dots stand out clearly against background.
        if max_luma > min_luma && (max_luma - min_luma) >= 20 {
            let range = (max_luma - min_luma) as f32;
            for pixel in gray.pixels_mut() {
                let val = pixel[0];
                let stretched = (((val.saturating_sub(min_luma)) as f32 / range) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                pixel[0] = stretched;
            }
        }

        // 4. Adaptive High-DPI Upscaling:
        // Desktop font heights are often 9-14px; Tesseract LSTM functions best with ~30-35px glyph heights.
        let scale_factor = if orig_h < 150 || orig_w < 200 {
            3
        } else if orig_h < 600 || orig_w < 800 {
            2
        } else {
            1
        };

        let scaled = if scale_factor > 1 {
            let target_w = orig_w.saturating_mul(scale_factor).min(4096);
            let target_h = orig_h.saturating_mul(scale_factor).min(4096);
            image::imageops::resize(
                &gray,
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            gray
        };

        // 5. Add 20px Solid White Border (Padding):
        let pad = 20u32;
        let padded_w = scaled.width() + pad * 2;
        let padded_h = scaled.height() + pad * 2;
        let mut padded = image::GrayImage::from_pixel(padded_w, padded_h, image::Luma([255]));

        image::imageops::overlay(&mut padded, &scaled, pad as i64, pad as i64);

        let final_img = image::DynamicImage::ImageLuma8(padded);

        let mut buffer = std::io::Cursor::new(Vec::new());
        if final_img
            .write_to(&mut buffer, image::ImageFormat::Png)
            .is_ok()
        {
            buffer.into_inner()
        } else {
            image_bytes.to_vec()
        }
    }

    /// Run tesseract on raw image bytes asynchronously with preprocessing, multi-pass PSM, and timeout protection
    pub async fn extract_text(image_bytes: &[u8]) -> Result<String, crate::error::CoreError> {
        use std::io::Write;

        if !Self::is_available().await {
            return Err(crate::error::CoreError::Storage(
                "Tesseract OCR is not installed. Please install 'tesseract-ocr' on your system."
                    .to_string(),
            ));
        }

        let preprocessed = Self::preprocess_image_bytes(image_bytes);

        let mut temp_file = tempfile::Builder::new()
            .prefix("clipboard-ocr-")
            .suffix(".png")
            .tempfile()
            .map_err(|e| {
                crate::error::CoreError::Storage(format!("Failed to create OCR tempfile: {}", e))
            })?;

        temp_file.write_all(&preprocessed).map_err(|e| {
            crate::error::CoreError::Storage(format!("Failed to write OCR tempfile: {}", e))
        })?;

        let temp_path = temp_file.path().to_path_buf();
        let installed_langs = Self::get_installed_languages().await;
        let lang_arg = Self::determine_ocr_languages(&installed_langs);
        let custom_tessdata = Self::get_custom_tessdata_dir();

        let run_tesseract = |psm: &str| {
            let mut cmd = tokio::process::Command::new("tesseract");
            if let Some(ref dir) = custom_tessdata {
                cmd.arg("--tessdata-dir").arg(dir);
            }
            cmd.arg(&temp_path).arg("stdout");
            if let Some(ref langs) = lang_arg {
                cmd.arg("-l").arg(langs);
            }
            cmd.arg("--dpi").arg("300");
            cmd.arg("--oem").arg("1");
            cmd.arg("--psm").arg(psm);
            cmd.arg("-c").arg("preserve_interword_spaces=1");
            cmd.kill_on_drop(true);
            cmd
        };

        // Pass 1: PSM 6 (Assume a single uniform block of text - optimal for clipboard snippets & UI captures)
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(25),
            run_tesseract("6").output(),
        )
        .await
        {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return Err(crate::error::CoreError::Storage(format!(
                    "Failed to execute tesseract: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(crate::error::CoreError::Storage(
                    "OCR process timed out after 25 seconds".to_string(),
                ));
            }
        };

        let mut text = if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            String::new()
        };

        // Pass 2 (Fallback): If PSM 6 returns empty or very short non-word text, try PSM 3 (standard layout analysis)
        if text.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
            let mut cmd3 = run_tesseract("3");
            if let Ok(Ok(retry_out)) =
                tokio::time::timeout(std::time::Duration::from_secs(15), cmd3.output()).await
            {
                if retry_out.status.success() {
                    let retry_text = String::from_utf8_lossy(&retry_out.stdout)
                        .trim()
                        .to_string();
                    if retry_text.chars().filter(|c| c.is_alphanumeric()).count() >= 2 {
                        text = retry_text;
                    }
                }
            }
        }

        // Pass 3 (Sparse Fallback): If still empty, try PSM 11 (Sparse text)
        if text.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
            let mut retry_cmd = run_tesseract("11");
            if let Ok(Ok(retry_out)) =
                tokio::time::timeout(std::time::Duration::from_secs(15), retry_cmd.output()).await
            {
                if retry_out.status.success() {
                    let retry_text = String::from_utf8_lossy(&retry_out.stdout)
                        .trim()
                        .to_string();
                    if !retry_text.is_empty() {
                        text = retry_text;
                    }
                }
            }
        }

        // Clean up repeated blank lines while preserving paragraphs
        let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
        let mut final_text = String::with_capacity(text.len());
        let mut prev_empty = false;
        for line in lines {
            let is_empty = line.trim().is_empty();
            if is_empty {
                if !prev_empty && !final_text.is_empty() {
                    final_text.push('\n');
                    prev_empty = true;
                }
            } else {
                if !final_text.is_empty() && !prev_empty {
                    final_text.push('\n');
                }
                final_text.push_str(line);
                prev_empty = false;
            }
        }

        Ok(final_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConcatDelimiter {
    Newline,
    DoubleNewline,
    Comma,
    Space,
    Semicolon,
    NumberedList,
    BulletList,
    Custom(String),
}

impl ConcatDelimiter {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Newline => "\n",
            Self::DoubleNewline => "\n\n",
            Self::Comma => ", ",
            Self::Space => " ",
            Self::Semicolon => "; ",
            Self::NumberedList => "\n",
            Self::BulletList => "\n",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Newline => "Newlines (\\n)",
            Self::DoubleNewline => "Paragraphs (\\n\\n)",
            Self::Comma => "Comma (, )",
            Self::Space => "Space ( )",
            Self::Semicolon => "Semicolon (; )",
            Self::NumberedList => "Numbered List (1. ...)",
            Self::BulletList => "Bulleted List (- ...)",
            Self::Custom(_) => "Custom Delimiter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffTag {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub tag: DiffTag,
    pub text: String,
    pub old_index: Option<usize>,
    pub new_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffResult {
    pub unified: String,
    pub additions: usize,
    pub deletions: usize,
    pub lines: Vec<DiffLine>,
}

pub struct DiffEngine;

impl DiffEngine {
    pub fn compute_diff(old_text: &str, new_text: &str) -> DiffResult {
        use similar::{ChangeTag, TextDiff};

        let diff = TextDiff::from_lines(old_text, new_text);
        let mut additions = 0;
        let mut deletions = 0;
        let mut lines = Vec::new();

        let mut old_idx = 1;
        let mut new_idx = 1;

        for change in diff.iter_all_changes() {
            let tag = match change.tag() {
                ChangeTag::Equal => {
                    let line = DiffLine {
                        tag: DiffTag::Equal,
                        text: change
                            .value()
                            .trim_end_matches(&['\r', '\n'][..])
                            .to_string(),
                        old_index: Some(old_idx),
                        new_index: Some(new_idx),
                    };
                    old_idx += 1;
                    new_idx += 1;
                    line
                }
                ChangeTag::Delete => {
                    deletions += 1;
                    let line = DiffLine {
                        tag: DiffTag::Delete,
                        text: change
                            .value()
                            .trim_end_matches(&['\r', '\n'][..])
                            .to_string(),
                        old_index: Some(old_idx),
                        new_index: None,
                    };
                    old_idx += 1;
                    line
                }
                ChangeTag::Insert => {
                    additions += 1;
                    let line = DiffLine {
                        tag: DiffTag::Insert,
                        text: change
                            .value()
                            .trim_end_matches(&['\r', '\n'][..])
                            .to_string(),
                        old_index: None,
                        new_index: Some(new_idx),
                    };
                    new_idx += 1;
                    line
                }
            };
            lines.push(tag);
        }

        let unified = diff
            .unified_diff()
            .context_radius(3)
            .header("Original", "Modified")
            .to_string();

        DiffResult {
            unified,
            additions,
            deletions,
            lines,
        }
    }
}

static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s<>]+").unwrap());

const TRACKING_PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
    "utm_source_platform",
    "utm_creative_format",
    "utm_marketing_tactic",
    "utm_cid",
    "utm_reader",
    "utm_referrer",
    "utm_name",
    "utm_social",
    "utm_social-type",
    "fbclid",
    "fb_action_ids",
    "fb_action_types",
    "fb_source",
    "fb_ref",
    "igshid",
    "gclid",
    "gclsrc",
    "dclid",
    "gad_source",
    "gbraid",
    "wbraid",
    "_ga",
    "_gl",
    "twclid",
    "si",
    "feature",
    "_r",
    "_t",
    "tt_from",
    "tt_medium",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "recipient_id",
    "vero_id",
    "vero_conv",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "yclid",
    "ym_debug",
    "ref",
    "ref_src",
    "ref_",
    "source",
    "campaign",
    "trk",
    "tracking_id",
    "aff_id",
    "affiliate_id",
    "spjobid",
    "spmailingid",
    "spreportid",
];

pub struct UrlCleaner;

impl UrlCleaner {
    /// Checks if a string is a standard HTTP/HTTPS URL.
    pub fn is_url(s: &str) -> bool {
        let trimmed = s.trim();
        trimmed.starts_with("http://") || trimmed.starts_with("https://")
    }

    /// Determines whether a given query parameter is considered tracking/telemetry.
    pub fn is_tracking_param(domain: Option<&str>, param_name: &str) -> bool {
        let lower = param_name.to_lowercase();
        if lower.starts_with("utm_")
            || lower.starts_with("pf_rd_")
            || lower.starts_with("ref_")
            || lower.starts_with("fb_")
        {
            return true;
        }
        if TRACKING_PARAMS.contains(&lower.as_str()) {
            return true;
        }
        if let Some(dom) = domain {
            let dom_lower = dom.to_lowercase();
            if (dom_lower.contains("twitter.com") || dom_lower.contains("x.com"))
                && (lower == "s" || lower == "t")
            {
                return true;
            }
        }
        false
    }

    /// Strips tracking and telemetry parameters from a single URL while preserving valid query params and fragments.
    pub fn clean_tracking_params(raw_url: &str) -> String {
        let trimmed = raw_url.trim();
        if !Self::is_url(trimmed) && !trimmed.contains('?') {
            return trimmed.to_string();
        }

        // Split off fragment
        let (without_frag, frag) = match trimmed.find('#') {
            Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
            None => (trimmed, ""),
        };

        // Split base from query string
        let (base, query_opt) = match without_frag.find('?') {
            Some(idx) => (&without_frag[..idx], Some(&without_frag[idx + 1..])),
            None => (without_frag, None),
        };

        // Extract domain from base if available
        let domain = if let Some(idx) = base.find("://") {
            let after_scheme = &base[idx + 3..];
            let host = match after_scheme.find('/') {
                Some(end) => &after_scheme[..end],
                None => after_scheme,
            };
            Some(host)
        } else {
            None
        };

        // Clean Amazon /ref=... path segments
        let mut base_string = base.to_string();
        if let Some(dom) = domain {
            if dom.to_lowercase().contains("amazon.") {
                if let Some(ref_idx) = base_string.find("/ref=") {
                    base_string = base_string[..ref_idx].to_string();
                }
            }
        }

        let query = match query_opt {
            Some(q) => q,
            None => {
                return format!("{}{}", base_string, frag);
            }
        };

        let mut cleaned_params = Vec::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let key = match pair.find('=') {
                Some(idx) => &pair[..idx],
                None => pair,
            };
            if !Self::is_tracking_param(domain, key) {
                cleaned_params.push(pair);
            }
        }

        if cleaned_params.is_empty() {
            format!("{}{}", base_string, frag)
        } else {
            format!("{}?{}{}", base_string, cleaned_params.join("&"), frag)
        }
    }

    /// Finds all URLs within text and cleans tracking parameters from them.
    pub fn clean_text_urls(text: &str) -> String {
        let trimmed = text.trim();
        if Self::is_url(trimmed) && !trimmed.contains(char::is_whitespace) {
            return Self::clean_tracking_params(trimmed);
        }

        URL_REGEX
            .replace_all(text, |caps: &regex::Captures| {
                let match_str = &caps[0];
                let trailing_punct: String = match_str
                    .chars()
                    .rev()
                    .take_while(|c| matches!(c, '.' | ',' | ';' | ':' | ')' | ']' | '}'))
                    .collect();
                let trailing_punct: String = trailing_punct.chars().rev().collect();
                let actual_url = &match_str[..match_str.len() - trailing_punct.len()];
                let cleaned = Self::clean_tracking_params(actual_url);
                format!("{}{}", cleaned, trailing_punct)
            })
            .into_owned()
    }

    /// Returns true if the text contains any URLs with tracking parameters.
    pub fn has_tracking_params(text: &str) -> bool {
        Self::clean_text_urls(text) != text
    }
}
