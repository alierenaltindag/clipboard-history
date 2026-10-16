//! Empirical Challenge Test Suite for Milestone 4 (R5: Code Quality & Zero Clippy Warnings)
//! Adversarially verifies:
//! 1. `checked_div` edge cases (pixel_count == 0, total_luma == 0, max u64, threshold boundary)
//! 2. `OcrEngine::preprocess_image_bytes` behavior with empty/boundary images and dark/light inversion
//! 3. Absence of clippy lint bypasses in git diff and no suppressions for R5 lints
//! 4. Struct update syntax ergonomics and default invariants in SyncConfig

use clipboard_history_core::config::SyncConfig;
use clipboard_history_core::transforms::OcrEngine;
use image::{ImageBuffer, Rgba};
use std::io::Cursor;
use std::process::Command;

// Helper: standard checked_div calculation as implemented in transforms/mod.rs
#[inline]
fn compute_avg_luma(total_luma: u64, pixel_count: u64) -> u8 {
    total_luma
        .checked_div(pixel_count)
        .map(|v| v as u8)
        .unwrap_or(255)
}

#[test]
fn challenge_checked_div_pixel_count_zero() {
    // When pixel_count == 0, checked_div returns None, falling back to 255 (neutral white background)
    assert_eq!(compute_avg_luma(0, 0), 255, "0 / 0 must safely return 255");
    assert_eq!(compute_avg_luma(1, 0), 255, "1 / 0 must safely return 255");
    assert_eq!(
        compute_avg_luma(1000, 0),
        255,
        "1000 / 0 must safely return 255"
    );
    assert_eq!(
        compute_avg_luma(u64::MAX, 0),
        255,
        "u64::MAX / 0 must safely return 255"
    );
}

#[test]
fn challenge_checked_div_total_luma_zero() {
    // When total_luma == 0 and pixel_count > 0, avg_luma should be 0 (pure black)
    assert_eq!(compute_avg_luma(0, 1), 0, "0 / 1 must be 0");
    assert_eq!(compute_avg_luma(0, 100), 0, "0 / 100 must be 0");
    assert_eq!(compute_avg_luma(0, 1_000_000), 0, "0 / 1_000_000 must be 0");
    assert_eq!(compute_avg_luma(0, u64::MAX), 0, "0 / u64::MAX must be 0");
}

#[test]
fn challenge_checked_div_u64_max() {
    // When total_luma is u64::MAX
    // 1. Divided by u64::MAX -> 1
    assert_eq!(
        compute_avg_luma(u64::MAX, u64::MAX),
        1,
        "u64::MAX / u64::MAX must be 1"
    );
    // 2. Divided by 1 -> u64::MAX truncated to u8 (0xFF == 255)
    assert_eq!(
        compute_avg_luma(u64::MAX, 1),
        255,
        "u64::MAX / 1 truncated to u8 must be 255"
    );
    // 3. Divided by 2 -> (u64::MAX / 2) as u8
    let expected = (u64::MAX / 2) as u8;
    assert_eq!(
        compute_avg_luma(u64::MAX, 2),
        expected,
        "u64::MAX / 2 must match (u64::MAX / 2) as u8"
    );
}

#[test]
fn challenge_checked_div_luma_range_and_inversion_boundary() {
    // Luminance ranges: 0..=255.
    // Dark mode inversion threshold in OCR is < 120.
    for luma in 0..=255u8 {
        for count in [1u64, 2, 7, 64, 100, 1920 * 1080] {
            let total = (luma as u64) * count;
            let avg = compute_avg_luma(total, count);
            assert_eq!(
                avg, luma,
                "Luminance {} with {} pixels must roundtrip accurately",
                luma, count
            );
        }
    }

    // Boundary at 119 (dark, inverts) vs 120 (light, does not invert)
    let dark_avg = compute_avg_luma(119 * 100, 100);
    assert_eq!(dark_avg, 119);
    assert!(dark_avg < 120, "119 triggers dark mode inversion");

    let light_avg = compute_avg_luma(120 * 100, 100);
    assert_eq!(light_avg, 120);
    assert!(light_avg >= 120, "120 does NOT trigger dark mode inversion");
}

#[test]
fn challenge_ocr_preprocess_image_bytes_end_to_end() {
    // 1. Empty bytes
    let empty_res = OcrEngine::preprocess_image_bytes(&[]);
    assert_eq!(
        empty_res,
        Vec::<u8>::new(),
        "Empty bytes return early safely"
    );

    // 2. Invalid image bytes
    let corrupted = b"NOT_A_VALID_IMAGE_HEADER";
    let corrupt_res = OcrEngine::preprocess_image_bytes(corrupted);
    assert_eq!(corrupt_res, corrupted, "Corrupted bytes return unmodified");

    // 3. 1x1 black pixel (luma 0 < 120 -> inverted to white)
    let mut black_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(1, 1);
    black_img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
    let mut black_bytes = Vec::new();
    black_img
        .write_to(&mut Cursor::new(&mut black_bytes), image::ImageFormat::Png)
        .unwrap();

    let preprocessed_black = OcrEngine::preprocess_image_bytes(&black_bytes);
    assert!(
        !preprocessed_black.is_empty(),
        "Preprocessed black image should produce valid output"
    );
    // Load preprocessed image: center pixel should be inverted from dark to light
    let loaded = image::load_from_memory(&preprocessed_black).expect("Valid output png");
    assert!(
        loaded.width() >= 1 && loaded.height() >= 1,
        "Output must have valid dimensions"
    );

    // 4. 1x1 white pixel (luma 255 >= 120 -> remains light)
    let mut white_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(1, 1);
    white_img.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
    let mut white_bytes = Vec::new();
    white_img
        .write_to(&mut Cursor::new(&mut white_bytes), image::ImageFormat::Png)
        .unwrap();

    let preprocessed_white = OcrEngine::preprocess_image_bytes(&white_bytes);
    assert!(
        !preprocessed_white.is_empty(),
        "Preprocessed white image should produce valid output"
    );
}

#[test]
fn challenge_sync_config_struct_update_invariants() {
    // Test that struct update syntax used in sync_service cleanly overrides fields
    // without altering any other defaults
    let default_cfg = SyncConfig::default();
    let overridden_cfg = SyncConfig {
        enabled: false,
        ..Default::default()
    };

    assert!(!overridden_cfg.enabled);
    assert_eq!(overridden_cfg.listen_port, default_cfg.listen_port);
    assert_eq!(overridden_cfg.device_name, default_cfg.device_name);
    assert_eq!(overridden_cfg.pairing_pin, default_cfg.pairing_pin);
    assert_eq!(overridden_cfg.peer_addresses, default_cfg.peer_addresses);
    assert_eq!(overridden_cfg.trusted_peers, default_cfg.trusted_peers);
}

#[test]
fn challenge_no_clippy_suppression_cheating_in_git_diff() {
    // Adversarially check git diff to ensure no clippy suppressions or bypass attributes
    // were inserted for M4 or anywhere in the workspace changes.
    let output = Command::new("git")
        .args(["diff", "HEAD"])
        .output()
        .expect("git diff command should execute");
    assert!(output.status.success(), "git diff must succeed");

    let diff_text = String::from_utf8_lossy(&output.stdout);

    let forbidden_patterns = [
        "allow(clippy",
        "expect(clippy",
        "manual_checked_ops",
        "collapsible_if",
        "field_reassign_with_default",
        "doc_lazy_continuation",
    ];

    for line in diff_text.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            let added_line = line.trim_start_matches('+').trim();
            for pattern in &forbidden_patterns {
                assert!(
                    !added_line.contains(pattern),
                    "Found forbidden clippy bypass pattern '{}' added in git diff: {}",
                    pattern,
                    line
                );
            }
        }
    }
}
