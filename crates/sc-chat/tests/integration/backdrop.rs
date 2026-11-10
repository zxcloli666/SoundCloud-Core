use std::time::Duration;

use sc_chat::api::wire::{MemberChange, NewConversation};
use sc_chat::envelope::Content;
use sc_chat::media::BackdropStep;
use sc_chat::{
    AvatarChange, BackdropChange, ChatError, ConvId, ConversationBackdrop, ConversationKind,
    ConversationPatch, MediaKind, MemberRole, RoomBackdrop,
};
use uuid::Uuid;

use crate::dm::direct;
use crate::public::{PUBLIC, owner, seated, watch};
use crate::support::media::{fetch, picture, upload};
use crate::support::{Client, Stack, public_turn};

const STAR_CACHE: Duration = Duration::from_secs(62);
pub const WIDE: (u32, u32) = (640, 360);

fn preset_by(setter: &Client, id: &str) -> Option<ConversationBackdrop> {
    Some(ConversationBackdrop {
        look: RoomBackdrop::Preset { id: id.into() },
        by: setter.person.user_id.clone(),
    })
}

pub fn image_by(setter: &Client, media_id: Uuid) -> Option<ConversationBackdrop> {
    Some(ConversationBackdrop {
        look: RoomBackdrop::Image { media_id },
        by: setter.person.user_id.clone(),
    })
}

async fn preset(setter: &Client, conversation_id: ConvId, id: &str) -> Result<(), ChatError> {
    let step = BackdropStep::preset(setter.store(), conversation_id, id.into())?;
    match step.send(&setter.api).await? {
        Some(meta) => setter
            .send_content(conversation_id, &Content::Meta(Box::new(meta)))
            .await
            .map(|_| ()),
        None => Ok(()),
    }
}

pub async fn image(setter: &Client, conversation_id: ConvId, tint: u8) -> Uuid {
    let source = picture(WIDE.0, WIDE.1, tint);
    let uploaded = upload(setter, conversation_id, MediaKind::Backdrop, &source).await;
    let change = BackdropChange::Set {
        upload_id: uploaded.upload_id,
    };
    let meta = BackdropStep::set(setter.store(), conversation_id, &change)
        .unwrap()
        .send(&setter.api)
        .await
        .expect("the image backdrop is set")
        .expect("an image comes with a meta that carries its key");
    assert_eq!(
        setter
            .send_content(conversation_id, &Content::Meta(Box::new(meta)))
            .await,
        Ok(1)
    );
    uploaded.media_id
}

pub async fn shows(client: &Client, conversation_id: ConvId, look: &Option<ConversationBackdrop>) {
    client
        .sync_until("the backdrop reaches the device", |client| {
            client
                .conversation(conversation_id)
                .is_some_and(|chat| &chat.backdrop == look)
        })
        .await;
}

pub async fn opens(client: &Client, conversation_id: ConvId, media_id: Uuid) {
    client
        .sync_until("the backdrop key reaches the device", |client| {
            resolved(client, conversation_id) == Some(media_id)
        })
        .await;
    let attachment = client
        .store()
        .backdrop_image(conversation_id)
        .unwrap()
        .unwrap();
    let opened = fetch(client, conversation_id, &attachment).await;
    assert_eq!((opened.w, opened.h), WIDE);
    assert!(!opened.bytes.is_empty());
}

fn resolved(client: &Client, conversation_id: ConvId) -> Option<Uuid> {
    client
        .store()
        .backdrop_image(conversation_id)
        .unwrap()
        .and_then(|attachment| Uuid::parse_str(&attachment.media_id).ok())
}

pub async fn group(owner: &Client, members: &[&Client]) -> ConvId {
    let view = owner
        .api
        .create_conversation(&NewConversation {
            kind: ConversationKind::Group,
            members: members
                .iter()
                .map(|member| member.person.user_id.clone())
                .collect(),
            meta_ct: None,
            meta_epoch: None,
        })
        .await
        .unwrap();
    owner.engine.adopt(&view).await.unwrap();
    assert_eq!(owner.send(view.id, "welcome").await, Ok(1));
    for member in members {
        member
            .sync_until("a member gets the group", |member| {
                member.texts(view.id) == ["welcome"]
            })
            .await;
    }
    view.id
}

#[tokio::test]
#[ignore]
async fn a_star_member_sets_a_dm_preset_for_both_sides_and_a_free_member_is_refused() {
    let stack = Stack::from_env();
    let ann_person = stack.person("ann");
    stack.star(&ann_person);
    let ann = Client::up(&stack, &ann_person).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    stack.follow(&bob.person, &ann.person);
    let view = ann
        .api
        .create_conversation(&direct(&bob.person.user_id))
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    assert_eq!(ann.send(view.id, "hi bob").await, Ok(1));
    bob.sync_until("bob gets the chat", |bob| bob.texts(view.id) == ["hi bob"])
        .await;

    assert_eq!(
        preset(&bob, view.id, "aurora").await,
        Err(ChatError::BackdropStarRequired)
    );
    assert_eq!(
        preset(&ann, view.id, "no-such-look").await,
        Err(ChatError::Invalid)
    );
    assert_eq!(preset(&ann, view.id, "aurora").await, Ok(()));
    let look = preset_by(&ann, "aurora");
    for client in [&ann, &bob] {
        shows(client, view.id, &look).await;
    }

    let cleared = BackdropStep::set(ann.store(), view.id, &BackdropChange::Clear).unwrap();
    assert_eq!(cleared.send(&ann.api).await, Ok(None));
    for client in [&ann, &bob] {
        shows(client, view.id, &None).await;
    }
}

#[tokio::test]
#[ignore]
async fn a_group_admin_uploads_a_backdrop_a_member_opens_it_and_an_avatar_change_keeps_it() {
    let stack = Stack::from_env();
    let (ann_person, bob_person) = (stack.person("ann"), stack.person("bob"));
    stack.star(&ann_person);
    stack.star(&bob_person);
    let ann = Client::up(&stack, &ann_person).await;
    let bob = Client::up(&stack, &bob_person).await;
    let cid = Client::up(&stack, &stack.person("cid")).await;
    stack.follow(&bob.person, &ann.person);
    stack.follow(&cid.person, &ann.person);
    let id = group(&ann, &[&bob, &cid]).await;

    assert_eq!(preset(&bob, id, "aurora").await, Err(ChatError::NotMember));
    let promoted = MemberChange {
        role: Some(MemberRole::Admin),
        unban: false,
    };
    ann.api
        .update_member(id, &bob.person.user_id, &promoted)
        .await
        .unwrap();
    bob.sync_until("bob learns he is an admin", |bob| {
        bob.conversation(id)
            .is_some_and(|chat| chat.role == MemberRole::Admin)
    })
    .await;

    let media_id = image(&bob, id, 40).await;
    let look = image_by(&bob, media_id);
    for client in [&ann, &bob, &cid] {
        shows(client, id, &look).await;
        opens(client, id, media_id).await;
    }

    let source = picture(256, 256, 200);
    let avatar = upload(&ann, id, MediaKind::Avatar, &source).await;
    let patch = ConversationPatch {
        avatar: Some(AvatarChange::Set {
            upload_id: avatar.upload_id,
        }),
        ..ConversationPatch::default()
    };
    let meta = ann.store().next_meta(id, &patch).unwrap();
    assert_eq!(
        ann.send_content(id, &Content::Meta(Box::new(meta))).await,
        Ok(1)
    );
    for client in [&ann, &bob, &cid] {
        client
            .sync_until("the new avatar reaches the device", |client| {
                client
                    .conversation(id)
                    .is_some_and(|chat| chat.avatar_media_id == Some(avatar.media_id))
            })
            .await;
        assert_eq!(client.conversation(id).unwrap().backdrop, look);
        opens(client, id, media_id).await;
    }
    assert_eq!(ann.api.conversation(id).await.unwrap().backdrop, look);
}

#[tokio::test]
#[ignore]
async fn saved_takes_an_image_the_seventh_change_is_refused_and_a_lapsed_star_hides_it() {
    let stack = Stack::from_env();
    let person = stack.person("fay");
    stack.star(&person);
    let fay = Client::up(&stack, &person).await;
    let view = fay
        .api
        .create_conversation(&NewConversation {
            kind: ConversationKind::Saved,
            members: Vec::new(),
            meta_ct: None,
            meta_epoch: None,
        })
        .await
        .unwrap();
    fay.engine.adopt(&view).await.unwrap();

    let media_id = image(&fay, view.id, 90).await;
    let shown = image_by(&fay, media_id);
    shows(&fay, view.id, &shown).await;
    opens(&fay, view.id, media_id).await;
    let same = RoomBackdrop::Image { media_id };
    for _ in 0..3 {
        let answer = fay.api.set_backdrop(view.id, Some(&same)).await.unwrap();
        assert_eq!(answer.backdrop, shown);
    }

    for id in ["aurora", "vinyl", "dunes", "satin", "paper"] {
        assert_eq!(preset(&fay, view.id, id).await, Ok(()));
    }
    assert_eq!(
        preset(&fay, view.id, "watercolor").await,
        Err(ChatError::RequestLimit)
    );
    assert_eq!(preset(&fay, view.id, "paper").await, Ok(()));
    let look = preset_by(&fay, "paper");
    shows(&fay, view.id, &look).await;

    stack.lapse(&fay.person);
    tokio::time::sleep(STAR_CACHE).await;
    let lapsed = fay.engine.refresh_conversation(view.id).await.unwrap();
    assert_eq!(lapsed.backdrop, None);
    assert_eq!(
        preset(&fay, view.id, "aurora").await,
        Err(ChatError::BackdropStarRequired)
    );
    stack.star(&fay.person);
    tokio::time::sleep(STAR_CACHE).await;
    let back = fay.engine.refresh_conversation(view.id).await.unwrap();
    assert_eq!(back.backdrop, look);
}

#[tokio::test]
#[ignore]
async fn the_public_owner_sets_a_preset_without_star_and_a_plain_member_cannot() {
    let _turn = public_turn().await;
    let stack = Stack::from_env();
    stack.forget_limits(&format!("backdrop:*:{PUBLIC}"));
    let boss = owner(&stack).await;
    let gus = seated(&stack, "gus").await;
    let hal = seated(&stack, "hal").await;
    watch(&gus).await;

    assert_eq!(
        preset(&gus, PUBLIC, "aurora").await,
        Err(ChatError::NotMember)
    );
    let own_image = RoomBackdrop::Image {
        media_id: Uuid::new_v4(),
    };
    assert_eq!(
        boss.api.set_backdrop(PUBLIC, Some(&own_image)).await.err(),
        Some(ChatError::BackdropStarRequired)
    );
    assert_eq!(preset(&boss, PUBLIC, "candy-sky").await, Ok(()));
    let look = preset_by(&boss, "candy-sky");
    shows(&gus, PUBLIC, &look).await;
    assert_eq!(
        hal.engine
            .open_public()
            .await
            .unwrap()
            .conversation
            .backdrop,
        look
    );

    let cleared = BackdropStep::set(boss.store(), PUBLIC, &BackdropChange::Clear).unwrap();
    assert_eq!(cleared.send(&boss.api).await, Ok(None));
    shows(&gus, PUBLIC, &None).await;
}
