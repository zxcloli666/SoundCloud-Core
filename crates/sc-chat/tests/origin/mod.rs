use std::sync::Arc;

use sc_chat::channel::{Channel, OckPin};
use sc_chat::sc_chat_wire::ock::OckSecret;
use sc_chat::sc_chat_wire::prost::Message;
use sc_chat::sc_chat_wire::proto::{Header, Inner, InnerResponse};
use sc_chat::sc_chat_wire::seal::{ENC_LEN, ResponseKey, open_request};
use sc_chat::{Clock, Millis, TransportResponse, async_trait};

pub const NOW: Millis = 1_760_958_000_000;

pub struct FixedClock(pub Millis);

#[async_trait]
impl Clock for FixedClock {
    fn now_ms(&self) -> Millis {
        self.0
    }

    async fn sleep(&self, _ms: u64) {}
}

pub struct Origin {
    secrets: Vec<(u8, OckSecret)>,
}

pub struct Received {
    pub ock_v: u8,
    pub enc: [u8; ENC_LEN],
    pub inner: Inner,
    response_key: ResponseKey,
}

impl Origin {
    pub fn new() -> Self {
        let secret =
            || OckSecret::from_bytes(&sc_chat::sc_chat_wire::random_bytes::<32>().unwrap());
        Self {
            secrets: vec![(1, secret().unwrap()), (2, secret().unwrap())],
        }
    }

    pub fn pins(&self) -> Vec<OckPin> {
        self.secrets
            .iter()
            .map(|(version, secret)| (*version, secret.public()))
            .collect()
    }

    pub fn channel(&self) -> Channel {
        Channel::with_pins(Arc::new(FixedClock(NOW)), self.pins())
    }

    pub fn receive(&self, envelope: &[u8]) -> Received {
        let (_, secret) = self
            .secrets
            .iter()
            .find(|(version, _)| *version == envelope[1])
            .expect("the envelope names a known ock");
        let opened = open_request(envelope, secret).expect("the origin opens the envelope");
        Received {
            ock_v: opened.ock_v,
            enc: opened.enc,
            inner: Inner::decode(opened.inner.as_slice()).expect("inner decodes"),
            response_key: opened.response_key,
        }
    }
}

impl Received {
    pub fn answer(&self, status: u32, body: &str) -> TransportResponse {
        let answer = InnerResponse {
            status,
            headers: vec![Header {
                name: "content-type".into(),
                value: "application/json".into(),
            }],
            body: body.as_bytes().to_vec(),
        };
        let nonce = sc_chat::sc_chat_wire::random_bytes().unwrap();
        let sealed = self
            .response_key
            .seal(&self.enc, nonce, &answer.encode_to_vec())
            .unwrap();
        TransportResponse {
            status: 200,
            body: sealed,
            retry_after_ms: None,
        }
    }
}
