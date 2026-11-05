use std::fmt;

include!(concat!(env!("OUT_DIR"), "/scd.chat.v1.rs"));

impl fmt::Debug for Attachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attachment")
            .field("media_id", &self.media_id)
            .field("kind", &self.kind)
            .field("mime", &self.mime)
            .field("size", &self.size)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("thumbhash", &Hidden(self.thumbhash.len()))
            .field("key", &Hidden(self.key.len()))
            .field("nonce", &Hidden(self.nonce.len()))
            .field("sha256_plain", &Hidden(self.sha256_plain.len()))
            .field("sha256_cipher", &Hidden(self.sha256_cipher.len()))
            .field("thumb", &self.thumb)
            .finish()
    }
}

impl fmt::Debug for Thumb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thumb")
            .field("media_id", &self.media_id)
            .field("key", &Hidden(self.key.len()))
            .field("nonce", &Hidden(self.nonce.len()))
            .field("size", &self.size)
            .field("w", &self.w)
            .field("h", &self.h)
            .finish()
    }
}

struct Hidden(usize);

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} bytes>", self.0)
    }
}
