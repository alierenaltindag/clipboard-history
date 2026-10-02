use crate::error::{CoreError, Result};
use image::{imageops::FilterType, GenericImageView, ImageFormat};
use std::io::Cursor;

pub struct ThumbnailGenerator;

impl ThumbnailGenerator {
    pub const THUMBNAIL_MAX_WIDTH: u32 = 128;
    pub const THUMBNAIL_MAX_HEIGHT: u32 = 128;

    pub fn generate(image_bytes: &[u8]) -> Result<((u32, u32), Vec<u8>)> {
        let img = image::load_from_memory(image_bytes).map_err(CoreError::Image)?;
        let dimensions = img.dimensions();

        let thumb = img.resize(
            Self::THUMBNAIL_MAX_WIDTH,
            Self::THUMBNAIL_MAX_HEIGHT,
            FilterType::Triangle,
        );

        let mut buffer = Cursor::new(Vec::new());
        thumb
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(CoreError::Image)?;

        Ok((dimensions, buffer.into_inner()))
    }
}
