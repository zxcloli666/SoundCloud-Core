mod animate;
mod backdrop;
mod cipher;
mod error;
mod ladder;
mod motion;
mod open;
mod open_motion;
mod prepare;
pub mod queue;
mod refs;
mod source;
mod worker;

pub use backdrop::BackdropStep;
pub use cipher::{BLOCK, Blob, Role, open_blob, seal_blob, sealed_len};
pub use error::MediaError;
pub use open::{OpenedImage, open_image, open_thumb};
pub use open_motion::open_motion;
pub use prepare::{
    PreparedAvatar, PreparedBackdrop, PreparedImage, prepare_avatar, prepare_backdrop,
    prepare_image,
};
pub use refs::{image_id, post_media, thumb_id};
pub use worker::{DECODE_THREADS, DECODE_TIMEOUT, MediaWorker, PREPARE_TIMEOUT};

pub const IMAGE_SIDE: u32 = 1600;
pub const IMAGE_QUALITY: f32 = 60.0;
pub const IMAGE_CAP: usize = 600 * 1024;
pub const MOTION_SIDE: u32 = 480;
pub const MOTION_FRAMES: u32 = 150;
pub const MOTION_DURATION_MS: u32 = 10_000;
pub const MOTION_PIXELS: u64 = 480 * 480 * 150;
pub const MOTION_SOURCE_PIXELS: u64 = 240_000_000;
pub const THUMB_SIDE: u32 = 320;
pub const THUMB_QUALITY: f32 = 45.0;
pub const THUMB_CAP: usize = 24 * 1024;
pub const THUMB_SERVER_CAP: usize = 32 * 1024;
pub const AVATAR_SIDE: u32 = 512;
pub const AVATAR_SERVER_CAP: usize = 64 * 1024;
pub const BACKDROP_SIDE: u32 = 1920;
pub const BACKDROP_SERVER_CAP: usize = 600 * 1024;
pub const ALBUM_MAX: usize = 10;
pub const ROOM_ALBUM_MAX: usize = 4;
pub const URLS_MAX: usize = 100;
pub const DELETE_MAX: usize = 500;
pub const SOURCE_MAX_BYTES: usize = 48 * 1024 * 1024;
pub const SOURCE_MAX_PIXELS: u64 = 48_000_000;
pub const RECEIVE_MAX_SIDE: u32 = 2048;
pub const RECEIVE_MAX_PIXELS: u64 = 4_200_000;
pub const RECEIVE_THUMB_SIDE: u32 = 512;
pub const RECEIVE_BUFFER_BUDGET: usize = 64 * 1024 * 1024;
pub const MIME_WEBP: &str = "image/webp";
