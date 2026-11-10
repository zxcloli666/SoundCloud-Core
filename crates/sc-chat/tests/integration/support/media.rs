use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
use sc_chat::api::wire::MediaUrl;
use sc_chat::media::{self, MediaWorker, OpenedImage, queue};
use sc_chat::sc_chat_wire::proto::Attachment;
use sc_chat::store::{NewUpload, UploadObject};
use sc_chat::{ConvId, MediaKind};
use uuid::Uuid;

use super::Client;
use super::http::{self, Reply, Request};
use super::people::now_ms;

const STORE_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Uploaded {
    pub upload_id: Uuid,
    pub media_id: Uuid,
}

pub fn picture(w: u32, h: u32, tint: u8) -> Vec<u8> {
    let image = RgbImage::from_fn(w, h, |x, y| {
        Rgb([tint, (x * 255 / w) as u8, (y * 255 / h) as u8])
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(&image, w, h, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

pub async fn upload(
    client: &Client,
    conversation_id: ConvId,
    kind: MediaKind,
    source: &[u8],
) -> Uploaded {
    let (ciphertext, attachment) = match kind {
        MediaKind::Backdrop => {
            let prepared = media::prepare_backdrop(source).unwrap();
            (prepared.image, prepared.attachment)
        }
        MediaKind::Avatar => {
            let prepared = media::prepare_avatar(source).unwrap();
            (prepared.image, prepared.attachment)
        }
        MediaKind::Image => panic!("album uploads are not part of these scenarios"),
    };
    let store = client.store();
    let now = now_ms() as i64;
    let new = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id,
        kind,
        image_ref: "integration".into(),
        thumb_ref: None,
        attachment,
    };
    let upload_id = store.enqueue_upload(&new, now).unwrap();
    let targets = queue::reserve(&client.api, store, upload_id, now)
        .await
        .expect("the upload is reserved");
    let headers = targets
        .image
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let put = store_call("PUT", &targets.image.url, headers, ciphertext).await;
    assert!(
        put.status < 300,
        "the store refused the upload: {} {}",
        put.status,
        String::from_utf8_lossy(&put.body)
    );
    store
        .upload_object_done(upload_id, UploadObject::Image, now)
        .unwrap();
    let committed = queue::commit(&client.api, store, upload_id, now)
        .await
        .expect("the upload is committed");
    Uploaded {
        upload_id,
        media_id: committed.id,
    }
}

pub async fn fetch(
    client: &Client,
    conversation_id: ConvId,
    attachment: &Attachment,
) -> OpenedImage {
    let media_id = media::image_id(attachment).unwrap();
    let urls = queue::urls(&client.api, Some(conversation_id), &[media_id])
        .await
        .expect("the image has a link");
    let url = match urls.items.first() {
        Some(MediaUrl::Ok { url, .. }) => url.clone(),
        other => panic!("the image is not served: {other:?}"),
    };
    let got = store_call("GET", &url, Vec::new(), Vec::new()).await;
    assert_eq!(got.status, 200, "the store did not serve the image");
    media::open_image(&MediaWorker::default(), &got.body, attachment)
        .expect("the image opens with the key from the meta")
}

async fn store_call(method: &str, url: &str, headers: Vec<(&str, String)>, body: Vec<u8>) -> Reply {
    let rest = url
        .strip_prefix("http://")
        .expect("the stack store is plain http");
    let (host, path) = rest.split_once('/').expect("a presigned link has a path");
    let request = Request {
        method,
        path: &format!("/{path}"),
        headers,
        body,
        timeout: STORE_TIMEOUT,
    };
    http::send(&format!("http://{host}"), request)
        .await
        .unwrap_or_else(|error| panic!("{method} to the store: {error}"))
}
