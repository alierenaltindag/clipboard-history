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

    /// Run tesseract on raw image bytes asynchronously
    pub async fn extract_text(image_bytes: &[u8]) -> Result<String, crate::error::CoreError> {
        use std::io::Write;

        let mut temp_file = tempfile::Builder::new()
            .prefix("clipboard-ocr-")
            .suffix(".png")
            .tempfile()
            .map_err(|e| {
                crate::error::CoreError::Storage(format!("Failed to create OCR tempfile: {}", e))
            })?;

        temp_file.write_all(image_bytes).map_err(|e| {
            crate::error::CoreError::Storage(format!("Failed to write OCR tempfile: {}", e))
        })?;

        let temp_path = temp_file.path().to_path_buf();

        let output = tokio::process::Command::new("tesseract")
            .arg(&temp_path)
            .arg("stdout")
            .arg("-l")
            .arg("eng+osd")
            .output()
            .await
            .map_err(|e| {
                crate::error::CoreError::Storage(format!("Failed to run tesseract: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::CoreError::Storage(format!(
                "Tesseract failed: {}",
                stderr
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(text)
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
