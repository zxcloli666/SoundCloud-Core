use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, Delay, Frames, ImageDecoder, ImageFormat, RgbaImage};

use super::animate::{Shot, animate};
use super::cipher::sealed_len;
use super::{
    IMAGE_CAP, MOTION_DURATION_MS, MOTION_FRAMES, MOTION_SIDE, MOTION_SOURCE_PIXELS,
    SOURCE_MAX_PIXELS,
};

const QUALITIES: [f32; 3] = [60.0, 45.0, 30.0];
const SIDES: [u32; 3] = [MOTION_SIDE, 360, 240];
const STRIDES: [usize; 3] = [1, 2, 3];
const SHORTEST_MS: u32 = 10;
const SHORT_AS_MS: u32 = 100;

pub(super) struct Motion {
    pub webp: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub frames: u32,
    pub duration_ms: u32,
}

struct Frame {
    image: RgbaImage,
    ms: u32,
}

pub(super) fn prepare(bytes: &[u8]) -> Option<Motion> {
    let frames = frames(bytes);
    for stride in STRIDES {
        let picked = pick(&frames, stride);
        if picked.len() < 2 {
            return None;
        }
        if let Some(motion) = fit(&picked) {
            return Some(motion);
        }
    }
    None
}

fn frames(bytes: &[u8]) -> Vec<Frame> {
    let decoded = match image::guess_format(bytes) {
        Ok(ImageFormat::Gif) => GifDecoder::new(Cursor::new(bytes))
            .ok()
            .and_then(|decoder| canvas(&decoder).map(|pixels| (pixels, decoder.into_frames()))),
        Ok(ImageFormat::WebP) => WebPDecoder::new(Cursor::new(bytes))
            .ok()
            .filter(WebPDecoder::has_animation)
            .and_then(|decoder| canvas(&decoder).map(|pixels| (pixels, decoder.into_frames()))),
        _ => None,
    };
    match decoded {
        Some((pixels, frames)) => collect(frames, MOTION_SOURCE_PIXELS / pixels),
        None => Vec::new(),
    }
}

fn canvas(decoder: &impl ImageDecoder) -> Option<u64> {
    let (w, h) = decoder.dimensions();
    let pixels = u64::from(w) * u64::from(h);
    (pixels > 0 && pixels <= SOURCE_MAX_PIXELS).then_some(pixels)
}

fn collect(frames: Frames, budget: u64) -> Vec<Frame> {
    let mut kept: Vec<Frame> = Vec::new();
    let mut total = 0;
    for frame in frames.take(usize::try_from(budget).unwrap_or(usize::MAX)) {
        let Ok(frame) = frame else {
            break;
        };
        let ms = millis(frame.delay());
        total += ms;
        if total > MOTION_DURATION_MS {
            break;
        }
        let image = frame.into_buffer();
        let image = match image.width().max(image.height()) > MOTION_SIDE {
            true => scaled(&image, MOTION_SIDE),
            false => image,
        };
        kept.push(Frame { image, ms });
        if kept.len() > MOTION_FRAMES as usize {
            kept = halve(kept);
        }
    }
    kept
}

fn millis(delay: Delay) -> u32 {
    let (numer, denom) = delay.numer_denom_ms();
    match numer / denom.max(1) {
        ms if ms <= SHORTEST_MS => SHORT_AS_MS,
        ms => ms,
    }
}

fn scaled(image: &RgbaImage, side: u32) -> RgbaImage {
    let (w, h) = image.dimensions();
    let scale = f64::from(side) / f64::from(w.max(h));
    let w = ((f64::from(w) * scale).round() as u32).max(1);
    let h = ((f64::from(h) * scale).round() as u32).max(1);
    image::imageops::resize(image, w, h, FilterType::Triangle)
}

fn halve(frames: Vec<Frame>) -> Vec<Frame> {
    let mut merged: Vec<Frame> = Vec::with_capacity(frames.len() / 2 + 1);
    for (at, frame) in frames.into_iter().enumerate() {
        match (at % 2, merged.last_mut()) {
            (1, Some(last)) => last.ms += frame.ms,
            _ => merged.push(frame),
        }
    }
    merged
}

fn pick(frames: &[Frame], stride: usize) -> Vec<Shot<'_>> {
    frames
        .chunks(stride)
        .map(|run| Shot {
            image: &run[0].image,
            ms: run.iter().map(|frame| frame.ms).sum(),
        })
        .collect()
}

fn fit(picked: &[Shot]) -> Option<Motion> {
    let longest = picked[0].image.width().max(picked[0].image.height());
    let mut tried = 0;
    for side in SIDES {
        let side = side.min(longest);
        if side == tried {
            continue;
        }
        tried = side;
        let sized: Vec<RgbaImage> = match side < longest {
            true => picked.iter().map(|shot| scaled(shot.image, side)).collect(),
            false => Vec::new(),
        };
        let shots: Vec<Shot> = match sized.is_empty() {
            true => picked.to_vec(),
            false => sized
                .iter()
                .zip(picked)
                .map(|(image, shot)| Shot { image, ms: shot.ms })
                .collect(),
        };
        for quality in QUALITIES {
            let webp = animate(&shots, quality).ok()?;
            if sealed_len(webp.len()) <= IMAGE_CAP {
                return described(webp);
            }
        }
    }
    None
}

fn described(webp: Vec<u8>) -> Option<Motion> {
    let decoder = image_webp::WebPDecoder::new(Cursor::new(&webp)).ok()?;
    if !decoder.is_animated() {
        return None;
    }
    let (w, h) = decoder.dimensions();
    let frames = decoder.num_frames();
    let duration_ms = u32::try_from(decoder.loop_duration()).ok()?;
    drop(decoder);
    Some(Motion {
        webp,
        w,
        h,
        frames,
        duration_ms,
    })
}
