use std::sync::Arc;

use sc_chat_wire::msg::encode_padded;
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::{Bind, Envelope};
use serde_json::Value;
use uuid::Uuid;

use super::message::{open_at, seal_at};
use super::{Ck, DeviceKeys, MessageAt, open_message};
use crate::channel::{Caller, Channel, InnerAnswer, InnerCall, Sealed};
use crate::contracts::{
    ChatError, Clock, EmbedKind, MessageClass, Millis, SpanKind, Tier, TransportResponse,
};
use crate::envelope::{self, Content, decode, encode};
use crate::login::seal_bound;

const REQUEST: &str = include_str!("../../../sc-chat-wire/vectors/request.json");
const BIND: &str = include_str!("../../../sc-chat-wire/vectors/bind.json");
const CK: &str = include_str!("../../../sc-chat-wire/vectors/ck.json");
const MSG: &str = include_str!("../../../sc-chat-wire/vectors/msg.json");

struct At(Millis);

#[async_trait::async_trait]
impl Clock for At {
    fn now_ms(&self) -> Millis {
        self.0
    }

    async fn sleep(&self, _ms: u64) {}
}

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

fn cases(raw: &str) -> Vec<Value> {
    json(raw)["cases"].as_array().unwrap().clone()
}

fn hex(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn array<const N: usize>(value: &Value) -> [u8; N] {
    hex(value).try_into().unwrap()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap().to_owned()
}

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}

fn tier(value: &Value) -> Tier {
    match value.as_i64().unwrap() {
        1 => Tier::Direct,
        2 => Tier::Relay,
        3 => Tier::Pro,
        other => panic!("tier {other}"),
    }
}

fn bind_keys() -> DeviceKeys {
    let bind = json(BIND);
    DeviceKeys::from_seeds(array(&bind["auth_seed"]), array(&bind["box_secret"]))
}

fn sealed_case(case: &Value) -> Sealed {
    let inner = &case["inner"];
    let clock = At(inner["ts_ms"].as_i64().unwrap());
    let pin = (
        case["ock_v"].as_u64().unwrap() as u8,
        array(&case["ock_pub"]),
    );
    let channel = Channel::with_pins(Arc::new(clock), vec![pin]);
    let call = InnerCall {
        method: text(&inner["method"]),
        path: text(&inner["path"]),
        query: text(&inner["query"]),
        body: text(&inner["body"]).into_bytes(),
    };
    let tier = tier(&inner["tier"]);
    let ikm_e = array(&case["ikm_e"]);
    if inner["bind"].is_null() {
        let keys = DeviceKeys::from_seeds(array(&case["signer_seed"]), [1; 32]);
        let caller = Caller {
            session_id: inner["session_id"].as_str().unwrap(),
            device_id: uuid(&inner["device_id"]),
            keys: &keys,
        };
        return channel.seal_call(&call, &caller, tier, ikm_e).unwrap();
    }
    let keys = bind_keys();
    assert_eq!(keys.auth_public(), array(&case["signer_pub"]));
    let bind = Bind::decode(hex(&inner["bind"]).as_slice()).unwrap();
    let device_id = Uuid::parse_str(&bind.device_id).unwrap();
    assert_eq!(keys.bind(device_id, bind.ts_ms).unwrap(), bind);
    let session = inner["session_id"].as_str();
    seal_bound(&channel, &keys, bind, &call, session, tier, ikm_e).unwrap()
}

fn recorded_answer(case: &Value) -> InnerAnswer {
    let response = &case["response"];
    InnerAnswer {
        status: response["status"].as_u64().unwrap() as u16,
        headers: response["headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (text(&pair[0]), text(&pair[1])))
            .collect(),
        body: text(&response["body"]).into_bytes(),
    }
}

#[test]
fn every_request_vector_seals_to_the_recorded_envelope() {
    for case in cases(REQUEST) {
        let name = text(&case["name"]);
        let sealed = sealed_case(&case);
        assert_eq!(sealed.envelope, hex(&case["envelope"]), "{name}");
        assert_eq!(sealed.enc, array::<32>(&case["enc"]), "{name}");
        assert_eq!(
            sealed.response_key.as_bytes(),
            &array::<32>(&case["response_key"]),
            "{name}"
        );
    }
}

#[test]
fn every_recorded_answer_opens_to_the_recorded_response() {
    for case in cases(REQUEST) {
        let name = text(&case["name"]);
        let sealed = sealed_case(&case);
        let opened = sealed.open(&hex(&case["response_sealed"])).unwrap();
        assert_eq!(opened, recorded_answer(&case), "{name}");
        assert_eq!(opened.header("Content-Type"), Some("application/json"));
    }
}

#[test]
fn a_recorded_answer_reads_its_inner_error() {
    let cases = cases(REQUEST);
    let sync = sealed_case(&cases[0]);
    let clock = At(0);
    let channel = Channel::with_pins(Arc::new(clock), vec![]);
    let response = TransportResponse {
        status: 200,
        body: hex(&cases[0]["response_sealed"]),
        retry_after_ms: None,
    };
    let answer = channel.receive(&sync, &response).unwrap();
    assert_eq!(answer.error(), Some(ChatError::Disabled));
    let history = sealed_case(&cases[1]);
    let answer = history.open(&hex(&cases[1]["response_sealed"])).unwrap();
    assert!(answer.is_success());
    assert_eq!(answer.error(), None);
}

#[test]
fn a_tampered_recorded_answer_never_opens() {
    let case = &cases(REQUEST)[0];
    let sealed = sealed_case(case);
    let answer = hex(&case["response_sealed"]);
    for at in [0, 23, 24, answer.len() - 1] {
        let mut tampered = answer.clone();
        tampered[at] ^= 1;
        assert_eq!(
            sealed.open(&tampered).err(),
            Some(ChatError::Crypto),
            "byte {at}"
        );
    }
    assert_eq!(
        sealed.open(&answer[..answer.len() - 1]).err(),
        Some(ChatError::Crypto)
    );
    let other = sealed_case(&cases(REQUEST)[1]);
    assert_eq!(other.open(&answer).err(), Some(ChatError::Crypto));
}

#[test]
fn the_bind_vector_rebuilds_from_device_keys() {
    let vector = json(BIND);
    let keys = bind_keys();
    assert_eq!(keys.auth_public(), array(&vector["auth_pub"]));
    assert_eq!(keys.box_public().unwrap(), array(&vector["box_pub"]));
    let bind = keys
        .bind(
            uuid(&vector["device_id"]),
            vector["ts_ms"].as_u64().unwrap(),
        )
        .unwrap();
    assert_eq!(bind.sig, hex(&vector["sig"]));
    assert_eq!(bind.encode_to_vec(), hex(&vector["bind_bytes"]));
}

#[test]
fn the_ck_vector_opens_only_for_its_device_and_epoch() {
    let vector = json(CK);
    let keys = DeviceKeys::from_seeds([2; 32], array(&vector["box_secret"]));
    assert_eq!(keys.box_public().unwrap(), array(&vector["box_pub"]));
    let conv = uuid(&vector["conversation_id"]);
    let device = uuid(&vector["device_id"]);
    let epoch = vector["epoch"].as_u64().unwrap() as u32;
    let sealed = hex(&vector["sealed"]);
    let ck = keys.open_ck(conv, epoch, device, &sealed).unwrap();
    assert_eq!(ck.as_bytes(), &array::<32>(&vector["ck"]));
    let wrong = [
        keys.open_ck(conv, epoch + 1, device, &sealed),
        keys.open_ck(Uuid::nil(), epoch, device, &sealed),
        keys.open_ck(conv, epoch, Uuid::nil(), &sealed),
        DeviceKeys::from_seeds([2; 32], [3; 32]).open_ck(conv, epoch, device, &sealed),
    ];
    for opened in wrong {
        assert_eq!(opened.err(), Some(ChatError::Crypto));
    }
}

fn message_at(case: &Value) -> MessageAt {
    MessageAt {
        conversation_id: uuid(&case["conversation_id"]),
        epoch: case["epoch"].as_u64().unwrap() as u32,
        sender_device: uuid(&case["sender_device"]),
        client_id: uuid(&case["client_id"]),
        class: MessageClass::parse(case["class"].as_str().unwrap()).unwrap(),
    }
}

#[test]
fn every_message_vector_seals_and_opens_byte_for_byte() {
    for case in cases(MSG) {
        let name = text(&case["name"]);
        let ck = Ck::from_bytes(array(&case["ck"]));
        let at = message_at(&case);
        let plaintext = hex(&case["plaintext"]);
        let envelope = Envelope::decode(plaintext.as_slice()).unwrap();
        let body = seal_at(&ck, &at, array(&case["nonce"]), &envelope).unwrap();
        assert_eq!(body, hex(&case["body"]), "{name}");
        assert_eq!(open_at(&ck, &at, &body).unwrap(), envelope, "{name}");
        let moved = MessageAt {
            class: match at.class {
                MessageClass::Post => MessageClass::Edit,
                _ => MessageClass::Post,
            },
            ..at
        };
        assert_eq!(open_at(&ck, &moved, &body).err(), Some(ChatError::Crypto));
    }
}

#[test]
fn every_message_vector_decodes_and_encodes_back_to_its_plaintext() {
    for case in cases(MSG) {
        let name = text(&case["name"]);
        let plaintext = hex(&case["plaintext"]);
        let content = decode(&Envelope::decode(plaintext.as_slice()).unwrap()).unwrap();
        assert_ne!(content, Content::Unknown, "{name}");
        assert_eq!(content.class(), message_at(&case).class, "{name}");
        assert_eq!(
            encode_padded(&encode(&content).unwrap()),
            plaintext,
            "{name}"
        );
    }
}

#[test]
fn the_text_and_reaction_vectors_open_as_their_class_and_target() {
    for case in cases(MSG).into_iter().filter(|case| case["name"] != "meta") {
        let ck = Ck::from_bytes(array(&case["ck"]));
        let at = message_at(&case);
        let body = hex(&case["body"]);
        assert!(open_message(&ck, &at, &body).is_ok());
        let target = case["envelope"]["reaction"]["target"].as_i64();
        let content = envelope::open(&ck, &at, target, &body).unwrap();
        assert_eq!(content.target(), target);
        let wrong_target = Some(target.unwrap_or(0) + 1);
        assert_eq!(
            envelope::open(&ck, &at, wrong_target, &body).err(),
            Some(ChatError::Protocol)
        );
    }
}

#[test]
fn the_text_vector_is_the_described_post() {
    let case = &cases(MSG)[0];
    let described = &case["envelope"];
    let envelope = Envelope::decode(hex(&case["plaintext"]).as_slice()).unwrap();
    let Content::Post(post) = decode(&envelope).unwrap() else {
        panic!("the text vector is a post");
    };
    assert_eq!(post.text.text, text(&described["text"]["text"]));
    assert_eq!(post.text.spans.len(), 2);
    assert_eq!(post.text.spans[0].kind, SpanKind::Mention);
    assert_eq!(post.text.spans[0].user_id.as_deref(), Some("42"));
    assert_eq!((post.text.spans[1].start, post.text.spans[1].len), (22, 3));
    assert_eq!(post.reply_to, Some(118));
    assert_eq!(post.mentions, vec!["42".to_owned()]);
    let embed = &post.embeds[0];
    assert_eq!(embed.kind, EmbedKind::Track);
    assert_eq!(embed.urn, text(&described["embeds"][0]["urn"]));
    assert_eq!(embed.at_ms, Some(83_000));
    assert_eq!(embed.secret_token, None);
}

#[test]
fn the_meta_vector_is_group_meta() {
    let case = cases(MSG)
        .into_iter()
        .find(|case| case["name"] == "meta")
        .unwrap();
    let envelope = Envelope::decode(hex(&case["plaintext"]).as_slice()).unwrap();
    let Content::Meta(meta) = decode(&envelope).unwrap() else {
        panic!("the meta vector carries meta");
    };
    assert_eq!(meta.title, text(&case["envelope"]["meta"]["title"]));
    assert_eq!(
        meta.description,
        text(&case["envelope"]["meta"]["description"])
    );
    assert_eq!(meta.avatar_media_id, None);
}
