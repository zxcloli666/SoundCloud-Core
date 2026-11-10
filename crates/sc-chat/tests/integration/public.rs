use sc_chat::envelope::Content;
use sc_chat::{
    ChatError, ConvId, ConversationKind, DeliveryState, MemberRole, MemberState, Newcomer,
    PublicOpened,
};
use uuid::Uuid;

use crate::support::client::post;
use crate::support::{Client, Stack, public_turn};

pub const PUBLIC: ConvId = Uuid::from_u128(0x0000_0000_0000_7000_8000_7075_626c_6963);
const NEWCOMER_MS: i64 = 600_000;
const NEWCOMER_BODY_MAX: u32 = 512;

pub async fn seated(stack: &Stack, name: &str) -> Client {
    let client = Client::up(stack, &stack.person(name)).await;
    join(&client).await;
    client
}

pub async fn owner(stack: &Stack) -> Client {
    let client = Client::up(stack, &stack.public_owner()).await;
    join(&client).await;
    client
}

async fn join(client: &Client) -> PublicOpened {
    client.see_public().await.expect("the device gets the cap");
    client.engine.open_public().await.expect("the chat opens")
}

pub async fn watch(client: &Client) {
    client.api.public_watch().await.expect("the watch is kept");
    client.engine.set_visible(true);
    client.engine.set_focus(Some(PUBLIC));
    let last = client.api.conversation(PUBLIC).await.unwrap().last_seq;
    client
        .sync_until("the watcher catches up", |client| {
            client
                .conversation(PUBLIC)
                .is_some_and(|chat| chat.last_seq >= last)
        })
        .await;
}

pub fn slow_mode(stack: &Stack, seconds: u32) {
    stack.sql(&format!(
        "UPDATE chat.conversations SET settings = jsonb_set(settings, '{{slow_mode_s}}', '{seconds}') WHERE id = '{PUBLIC}'"
    ));
}

fn membership(stack: &Stack, client: &Client) -> String {
    stack.sql(&format!(
        "SELECT role || ' ' || state FROM chat.members WHERE conversation_id = '{PUBLIC}' AND user_id = '{}'",
        client.person.user_id
    ))
}

pub fn line(client: &Client, words: &str) -> String {
    format!("{words} {}", client.person.user_id)
}

fn has(client: &Client, text: &str) -> bool {
    client.message(PUBLIC, text).is_some()
}

pub fn last_state(client: &Client) -> Option<DeliveryState> {
    client
        .outbox
        .pending(PUBLIC)
        .unwrap()
        .last()
        .map(|message| message.state)
}

#[tokio::test]
#[ignore]
async fn the_public_chat_is_hidden_without_the_cap_and_the_first_open_joins() {
    let stack = Stack::from_env();
    let gus = Client::up(&stack, &stack.person("gus")).await;
    assert_eq!(
        gus.engine.open_public().await.err(),
        Some(ChatError::NotFound)
    );
    assert_eq!(
        gus.api.conversation(PUBLIC).await.err(),
        Some(ChatError::NotFound)
    );
    assert_eq!(
        gus.api.public_watch().await.err(),
        Some(ChatError::NotFound)
    );
    let listed = gus.api.conversations(None).await.unwrap();
    assert!(
        listed
            .items
            .iter()
            .all(|item| item.kind != ConversationKind::Public)
    );
    assert_eq!(membership(&stack, &gus), "");

    gus.see_public().await.unwrap();
    let listed = gus.api.conversations(None).await.unwrap();
    assert_eq!(listed.items[0].id, PUBLIC);
    assert_eq!(membership(&stack, &gus), "");
    let opened = gus.engine.open_public().await.unwrap();
    assert_eq!(opened.conversation.id, PUBLIC);
    assert_eq!(opened.conversation.kind, ConversationKind::Public);
    assert_eq!(opened.conversation.role, MemberRole::Member);
    assert_eq!(opened.conversation.state, MemberState::Active);
    assert_eq!(opened.mute, None);
    assert_eq!(membership(&stack, &gus), "member active");
    assert_eq!(
        gus.conversation(PUBLIC).map(|stored| stored.kind),
        Some(ConversationKind::Public)
    );
    gus.api.public_watch().await.unwrap();
    assert!(gus.engine.open_public().await.unwrap().online >= 1);
}

#[tokio::test]
#[ignore]
async fn two_members_post_and_read_and_slow_mode_holds_the_second_post() {
    let _turn = public_turn().await;
    let stack = Stack::from_env();
    let boss = owner(&stack).await;
    let ann = seated(&stack, "ann").await;
    let bob = seated(&stack, "bob").await;
    slow_mode(&stack, 5);
    for client in [&ann, &bob] {
        watch(client).await;
    }
    boss.api.public_slow_mode(0).await.unwrap();
    ann.sync_until("ann sees slow mode off", |ann| {
        ann.conversation(PUBLIC)
            .is_some_and(|chat| chat.slow_mode_s == 0)
    })
    .await;

    let (hello, answer) = (line(&ann, "hello from ann"), line(&bob, "hello from bob"));
    assert_eq!(ann.send(PUBLIC, &hello).await, Ok(1));
    bob.sync_until("bob reads ann", |bob| has(bob, &hello))
        .await;
    assert_eq!(bob.send(PUBLIC, &answer).await, Ok(1));
    ann.sync_until("ann reads bob", |ann| has(ann, &answer))
        .await;
    assert!(!ann.message(PUBLIC, &answer).unwrap().mine);

    assert_eq!(
        ann.api.public_slow_mode(30).await.err(),
        Some(ChatError::NotMember)
    );
    boss.api.public_slow_mode(30).await.unwrap();
    for client in [&ann, &bob] {
        client
            .sync_until("a member sees slow mode on", |client| {
                client
                    .conversation(PUBLIC)
                    .is_some_and(|chat| chat.slow_mode_s == 30)
            })
            .await;
    }
    let (first, held) = (line(&ann, "first under slow mode"), line(&ann, "held back"));
    assert_eq!(ann.send(PUBLIC, &first).await, Ok(1));
    match ann.raw_room_post(PUBLIC).await {
        Err(ChatError::SlowMode { retry_ms }) => assert!((1..=30_000).contains(&retry_ms)),
        other => panic!("slow mode let a second post through: {other:?}"),
    }
    assert_eq!(ann.send(PUBLIC, &held).await, Ok(0));
    assert_eq!(last_state(&ann), Some(DeliveryState::Pending));
    let (one, two) = (line(&boss, "staff one"), line(&boss, "staff two"));
    assert_eq!(boss.send(PUBLIC, &one).await, Ok(1));
    assert_eq!(boss.send(PUBLIC, &two).await, Ok(1));
    bob.sync_until("bob reads what got through", |bob| {
        has(bob, &first) && has(bob, &one) && has(bob, &two)
    })
    .await;
    assert!(!has(&bob, &held));
    boss.api.public_slow_mode(0).await.unwrap();
}

fn longest_fitting(newcomer: &Newcomer) -> String {
    let mut text = "a".repeat(newcomer.body_max as usize);
    while !newcomer.fits(&text) {
        text.pop();
    }
    while newcomer.fits(&format!("{text}a")) {
        text.push('a');
    }
    text
}

#[tokio::test]
#[ignore]
async fn a_newcomer_window_caps_the_post_size_exactly_where_the_client_says() {
    let _turn = public_turn().await;
    let stack = Stack::from_env();
    slow_mode(&stack, 0);
    let eve = Client::up(&stack, &stack.person("eve")).await;
    let joined = join(&eve).await;
    let joined_at: i64 = stack
        .sql(&format!(
            "SELECT floor(extract(epoch FROM joined_at) * 1000)::bigint FROM chat.members WHERE conversation_id = '{PUBLIC}' AND user_id = '{}'",
            eve.person.user_id
        ))
        .parse()
        .unwrap();
    let newcomer = joined.newcomer.expect("a fresh member is a newcomer");
    assert_eq!(
        newcomer,
        Newcomer {
            until: Some(joined_at + NEWCOMER_MS),
            body_max: NEWCOMER_BODY_MAX,
        }
    );

    let fitting = longest_fitting(&newcomer);
    let over = format!("{fitting}a");
    assert!(newcomer.fits(&fitting) && !newcomer.fits(&over));
    assert_eq!(eve.send(PUBLIC, &over).await, Ok(0));
    assert_eq!(last_state(&eve), Some(DeliveryState::Failed));
    assert_eq!(
        eve.raw_room_post_of(PUBLIC, 4_096).await.err(),
        Some(ChatError::SendLimited)
    );
    assert_eq!(eve.send(PUBLIC, &fitting).await, Ok(1));

    stack.sql(&format!(
        "UPDATE chat.members SET joined_at = NULL WHERE conversation_id = '{PUBLIC}' AND user_id = '{}'",
        eve.person.user_id
    ));
    assert_eq!(
        eve.engine.open_public().await.unwrap().newcomer,
        Some(Newcomer {
            until: None,
            body_max: NEWCOMER_BODY_MAX,
        })
    );
    stack.sql(&format!(
        "UPDATE chat.members SET joined_at = now() - interval '11 minutes' WHERE conversation_id = '{PUBLIC}' AND user_id = '{}'",
        eve.person.user_id
    ));
    assert_eq!(eve.engine.open_public().await.unwrap().newcomer, None);
    assert_eq!(eve.send(PUBLIC, &over).await, Ok(1));
}

#[tokio::test]
#[ignore]
async fn a_mention_and_a_reply_ping_their_target_and_nobody_else() {
    let _turn = public_turn().await;
    let stack = Stack::from_env();
    slow_mode(&stack, 0);
    let ann = seated(&stack, "ann").await;
    let bob = seated(&stack, "bob").await;
    let cid = seated(&stack, "cid").await;
    let pinged = |client: &Client| client.conversation(PUBLIC).is_some_and(|chat| chat.pinged);

    let mut mention = post(&line(&ann, "look here"));
    mention.mentions = vec![bob.person.user_id.clone()];
    assert_eq!(
        ann.send_content(PUBLIC, &Content::Post(mention)).await,
        Ok(1)
    );
    bob.sync_until("the mention pings bob", pinged).await;
    assert!(!cid.engine.open_public().await.unwrap().conversation.pinged);

    let seen = bob.engine.open_public().await.unwrap().conversation;
    assert!(seen.pinged);
    bob.engine.queue_read(PUBLIC, seen.last_seq);
    bob.sync().await.unwrap();
    assert!(!bob.engine.open_public().await.unwrap().conversation.pinged);

    let base = line(&bob, "reply to this");
    assert_eq!(bob.send(PUBLIC, &base).await, Ok(1));
    watch(&ann).await;
    assert!(has(&ann, &base));
    let mut reply = post(&line(&ann, "replying"));
    reply.reply_to = ann.message(PUBLIC, &base).and_then(|message| message.seq);
    assert!(reply.reply_to.is_some());
    assert_eq!(ann.send_content(PUBLIC, &Content::Post(reply)).await, Ok(1));
    bob.sync_until("the reply pings bob", pinged).await;
    assert!(!cid.engine.open_public().await.unwrap().conversation.pinged);
}
