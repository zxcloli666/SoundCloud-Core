use std::ptr;

use image::RgbaImage;
use libwebp_sys as sys;

use super::MediaError;

const ABI: i32 = sys::WEBP_MUX_ABI_VERSION as i32;

#[derive(Clone, Copy)]
pub(super) struct Shot<'a> {
    pub image: &'a RgbaImage,
    pub ms: u32,
}

pub(super) fn animate(shots: &[Shot], quality: f32) -> Result<Vec<u8>, MediaError> {
    let first = shots.first().ok_or(MediaError::Encode)?;
    let mut config = sys::WebPConfig::new().map_err(|_| MediaError::Encode)?;
    config.quality = quality;
    config.method = 2;
    config.alpha_quality = 70;
    let mut encoder = Encoder::new(first.image.width(), first.image.height())?;
    let mut at = 0;
    for shot in shots {
        encoder.add(shot.image, at, &config)?;
        at = at.saturating_add(i32::try_from(shot.ms).map_err(|_| MediaError::Encode)?);
    }
    encoder.finish(at)
}

struct Encoder(*mut sys::WebPAnimEncoder);

impl Encoder {
    fn new(w: u32, h: u32) -> Result<Self, MediaError> {
        let w = i32::try_from(w).map_err(|_| MediaError::Encode)?;
        let h = i32::try_from(h).map_err(|_| MediaError::Encode)?;
        let mut options = std::mem::MaybeUninit::uninit();
        let encoder = unsafe {
            if sys::WebPAnimEncoderOptionsInitInternal(options.as_mut_ptr(), ABI) == 0 {
                return Err(MediaError::Encode);
            }
            sys::WebPAnimEncoderNewInternal(w, h, options.as_ptr(), ABI)
        };
        match encoder.is_null() {
            true => Err(MediaError::Encode),
            false => Ok(Self(encoder)),
        }
    }

    fn add(
        &mut self,
        image: &RgbaImage,
        at: i32,
        config: &sys::WebPConfig,
    ) -> Result<(), MediaError> {
        let mut picture = sys::WebPPicture::new().map_err(|_| MediaError::Encode)?;
        picture.use_argb = 1;
        picture.width = i32::try_from(image.width()).map_err(|_| MediaError::Encode)?;
        picture.height = i32::try_from(image.height()).map_err(|_| MediaError::Encode)?;
        let added = unsafe {
            let imported =
                sys::WebPPictureImportRGBA(&mut picture, image.as_ptr(), picture.width * 4) != 0;
            let added = imported && sys::WebPAnimEncoderAdd(self.0, &mut picture, at, config) != 0;
            sys::WebPPictureFree(&mut picture);
            added
        };
        match added {
            true => Ok(()),
            false => Err(MediaError::Encode),
        }
    }

    fn finish(&mut self, end: i32) -> Result<Vec<u8>, MediaError> {
        let mut data = sys::WebPData::default();
        unsafe {
            if sys::WebPAnimEncoderAdd(self.0, ptr::null_mut(), end, ptr::null()) == 0
                || sys::WebPAnimEncoderAssemble(self.0, &mut data) == 0
            {
                return Err(MediaError::Encode);
            }
            let webp = std::slice::from_raw_parts(data.bytes, data.size).to_vec();
            sys::WebPDataClear(&mut data);
            Ok(webp)
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { sys::WebPAnimEncoderDelete(self.0) }
    }
}
