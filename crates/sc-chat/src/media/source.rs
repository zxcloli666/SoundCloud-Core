use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use qcms::{DataType, Intent, Profile, Transform};

use super::{MediaError, SOURCE_MAX_BYTES, SOURCE_MAX_PIXELS};

const HEIF_BRANDS: [&[u8]; 8] = [
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1",
];

pub(super) fn decode(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>), MediaError> {
    if bytes.len() > SOURCE_MAX_BYTES {
        return Err(MediaError::TooLarge);
    }
    if bytes.get(4..8) == Some(b"ftyp") && bytes.get(8..12).is_some_and(is_heif_brand) {
        return Err(MediaError::Heic);
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| MediaError::Malformed)?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP)
    ) {
        return Err(MediaError::Unsupported);
    }
    let mut decoder = reader.into_decoder().map_err(|_| MediaError::Malformed)?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > SOURCE_MAX_PIXELS {
        return Err(MediaError::Dimensions);
    }
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| MediaError::Malformed)?;
    image.apply_orientation(orientation);
    Ok((flatten(image), icc))
}

pub(super) fn to_srgb(image: DynamicImage, profile: &[u8]) -> DynamicImage {
    let Some(input) = Profile::new_from_slice(profile, false) else {
        return image;
    };
    let output = Profile::new_sRGB();
    match image {
        DynamicImage::ImageRgb8(mut pixels) => {
            if let Some(transform) =
                Transform::new(&input, &output, DataType::RGB8, Intent::default())
            {
                let source = pixels.as_raw().clone();
                transform.convert(&source, &mut pixels);
            }
            DynamicImage::ImageRgb8(pixels)
        }
        DynamicImage::ImageRgba8(mut pixels) => {
            if let Some(transform) =
                Transform::new(&input, &output, DataType::RGBA8, Intent::default())
            {
                let source = pixels.as_raw().clone();
                transform.convert(&source, &mut pixels);
            }
            DynamicImage::ImageRgba8(pixels)
        }
        other => other,
    }
}

fn is_heif_brand(brand: &[u8]) -> bool {
    HEIF_BRANDS.contains(&brand)
}

fn flatten(image: DynamicImage) -> DynamicImage {
    if !image.color().has_alpha() {
        return DynamicImage::ImageRgb8(image.into_rgb8());
    }
    let rgba = image.into_rgba8();
    match rgba.pixels().all(|pixel| pixel.0[3] == u8::MAX) {
        true => DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).into_rgb8()),
        false => DynamicImage::ImageRgba8(rgba),
    }
}
