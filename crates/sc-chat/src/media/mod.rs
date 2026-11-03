mod cipher;
mod error;
mod ladder;
mod open;
mod prepare;
mod source;
mod worker;

pub use cipher::{BLOCK, Blob, Role, open_blob, seal_blob, sealed_len};
pub use error::MediaError;
pub use open::{OpenedImage, open_image, open_thumb};
pub use prepare::{PreparedImage, prepare_image};
pub use worker::{DECODE_THREADS, DECODE_TIMEOUT, MediaWorker};

pub const IMAGE_SIDE: u32 = 1600;
pub const IMAGE_QUALITY: f32 = 60.0;
pub const IMAGE_CAP: usize = 600 * 1024;
pub const THUMB_SIDE: u32 = 320;
pub const THUMB_QUALITY: f32 = 45.0;
pub const THUMB_CAP: usize = 24 * 1024;
pub const THUMB_SERVER_CAP: usize = 32 * 1024;
pub const SOURCE_MAX_BYTES: usize = 48 * 1024 * 1024;
pub const SOURCE_MAX_PIXELS: u64 = 48_000_000;
pub const RECEIVE_MAX_SIDE: u32 = 2048;
pub const RECEIVE_MAX_PIXELS: u64 = 4_200_000;
pub const RECEIVE_THUMB_SIDE: u32 = 512;
pub const RECEIVE_BUFFER_BUDGET: usize = 64 * 1024 * 1024;
pub const MIME_WEBP: &str = "image/webp";
