use image::DynamicImage;
use image::imageops::FilterType;
use webp::{Encoder, PixelLayout, WebPConfig};

use super::MediaError;
use super::cipher::sealed_len;

pub(super) struct Ladder {
    pub sides: &'static [u32],
    pub qualities: &'static [f32],
    pub cap: usize,
}

pub(super) struct Encoded {
    pub webp: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

pub(super) const IMAGE: Ladder = Ladder {
    sides: &[1600, 1280, 1024, 800, 640],
    qualities: &[60.0, 50.0, 40.0, 30.0],
    cap: super::IMAGE_CAP,
};

pub(super) const AVATAR: Ladder = Ladder {
    sides: &[512, 384, 256],
    qualities: &[70.0, 60.0, 50.0, 40.0],
    cap: super::AVATAR_SERVER_CAP,
};

pub(super) const THUMB: Ladder = Ladder {
    sides: &[320, 256, 192, 128],
    qualities: &[45.0, 35.0, 25.0, 15.0],
    cap: super::THUMB_CAP,
};

pub(super) fn fit_side(image: &DynamicImage, side: u32) -> DynamicImage {
    match image.width().max(image.height()) > side {
        true => image.resize(side, side, FilterType::Lanczos3),
        false => image.clone(),
    }
}

pub(super) fn encode(image: &DynamicImage, ladder: &Ladder) -> Result<Encoded, MediaError> {
    let mut tried = 0;
    for &side in ladder.sides {
        let longest = image.width().max(image.height());
        let side = side.min(longest);
        if side == tried {
            continue;
        }
        tried = side;
        let frame = fit_side(image, side);
        for &quality in ladder.qualities {
            let webp = lossy(&frame, quality)?;
            if sealed_len(webp.len()) <= ladder.cap {
                return Ok(Encoded {
                    w: frame.width(),
                    h: frame.height(),
                    webp,
                });
            }
        }
    }
    Err(MediaError::NoFit)
}

fn lossy(frame: &DynamicImage, quality: f32) -> Result<Vec<u8>, MediaError> {
    let (w, h) = (frame.width(), frame.height());
    let encoder = match frame {
        DynamicImage::ImageRgb8(pixels) => Encoder::new(pixels, PixelLayout::Rgb, w, h),
        DynamicImage::ImageRgba8(pixels) => Encoder::new(pixels, PixelLayout::Rgba, w, h),
        _ => return Err(MediaError::Encode),
    };
    let mut config = WebPConfig::new().map_err(|_| MediaError::Encode)?;
    config.quality = quality;
    config.method = 5;
    config.alpha_quality = 70;
    let webp = encoder
        .encode_advanced(&config)
        .map_err(|_| MediaError::Encode)?;
    Ok(webp.to_vec())
}
