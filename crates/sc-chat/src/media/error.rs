use crate::contracts::ChatError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    #[error("the image format is not supported")]
    Unsupported,
    #[error("HEIC images are not supported")]
    Heic,
    #[error("the image is malformed")]
    Malformed,
    #[error("animated images are not supported")]
    Animated,
    #[error("the file is larger than the limit")]
    TooLarge,
    #[error("the image dimensions are over the limit")]
    Dimensions,
    #[error("the image does not fit the size cap")]
    NoFit,
    #[error("the image disagrees with its attachment")]
    Mismatch,
    #[error("the ciphertext does not open")]
    Cipher,
    #[error("the decode workers are busy")]
    Busy,
    #[error("decoding took too long")]
    TimedOut,
    #[error("encoding failed")]
    Encode,
    #[error("the system has no randomness")]
    Random,
}

impl From<MediaError> for ChatError {
    fn from(error: MediaError) -> Self {
        match error {
            MediaError::Cipher | MediaError::Random => Self::Crypto,
            MediaError::Busy | MediaError::TimedOut => Self::NotReady,
            MediaError::Mismatch | MediaError::Malformed => Self::Protocol,
            _ => Self::Invalid,
        }
    }
}
