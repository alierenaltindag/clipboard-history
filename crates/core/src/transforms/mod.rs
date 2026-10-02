use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use qrcode::render::svg;
use qrcode::QrCode;
use regex::Regex;
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
}
