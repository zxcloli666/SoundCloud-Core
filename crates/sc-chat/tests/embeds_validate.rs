use sc_chat::embeds::wire::{from_proto, sanitize, to_proto};
use sc_chat::embeds::{TRACKS_MAX, validate, validate_all};
use sc_chat::sc_chat_wire::proto::{self, Envelope, Suggest, envelope::Body};
use sc_chat::{EMBED_BYTES_MAX, Embed, EmbedKind, EmbedTrack, RoomInvite};

fn track() -> Embed {
    Embed {
        kind: EmbedKind::Track,
        urn: "soundcloud:tracks:123".into(),
        permalink: "https://soundcloud.com/artist/song".into(),
        secret_token: Some("s-AbC_9".into()),
        title: "Song".into(),
        artist: Some("Artist".into()),
        artwork_url: Some("https://i1.sndcdn.com/artworks-x-t500x500.jpg".into()),
        duration_ms: Some(200_000),
        access: Some("playable".into()),
        at_ms: Some(83_000),
        tracks: Vec::new(),
        room: None,
    }
}

fn listed(n: usize) -> EmbedTrack {
    EmbedTrack {
        urn: format!("soundcloud:tracks:{n}"),
        title: format!("Track {n}"),
        artist: "Artist".into(),
        duration_ms: 1_000,
        artwork_url: Some("https://i1.sndcdn.com/a.jpg".into()),
    }
}

#[test]
fn a_clean_snapshot_passes_unchanged() {
    assert_eq!(validate(&track()), Ok(track()));
}

#[test]
fn artwork_must_be_https_on_sndcdn() {
    for bad in [
        "http://i1.sndcdn.com/a.jpg",
        "https://evil.com/a.jpg",
        "https://sndcdn.com.evil.com/a.jpg",
        "https://evilsndcdn.com/a.jpg",
        "https://user@i1.sndcdn.com/a.jpg",
        "https://i1.sndcdn.com:444/a.jpg",
        "https://I1.SNDCDN.COM/a.jpg",
        "https://i1.sndcdn.com/a b.jpg",
        "https://i1.sndcdn.com/a\".jpg",
        "javascript:alert(1)",
    ] {
        let mut embed = track();
        embed.artwork_url = Some(bad.into());
        assert_eq!(validate(&embed).unwrap().artwork_url, None, "{bad}");
    }
    let mut bare = track();
    bare.artwork_url = Some("https://sndcdn.com/a.jpg".into());
    assert!(validate(&bare).unwrap().artwork_url.is_some());
}

#[test]
fn links_off_soundcloud_are_dropped() {
    let mut embed = track();
    embed.permalink = "https://soundcloud.com.evil.com/x".into();
    assert_eq!(validate(&embed).unwrap().permalink, "");
    embed.permalink = "https://on.soundcloud.com/abc".into();
    assert_eq!(
        validate(&embed).unwrap().permalink,
        "https://on.soundcloud.com/abc"
    );
}

#[test]
fn text_is_trimmed_cut_and_stripped_of_controls() {
    let mut embed = track();
    embed.title = format!("  \u{202e}Ti\u{0007}tle{}  ", "x".repeat(300));
    embed.artist = Some(" \n ".into());
    let clean = validate(&embed).unwrap();
    assert!(clean.title.starts_with("Title"));
    assert_eq!(clean.title.chars().count(), 120);
    assert_eq!(clean.artist, None);
}

#[test]
fn broken_snapshots_are_refused() {
    let mut no_title = track();
    no_title.title = " ".into();
    assert!(validate(&no_title).is_err());
    let mut bad_urn = track();
    bad_urn.urn = "soundcloud:tracks:1;drop".into();
    assert!(validate(&bad_urn).is_err());
    let mut foreign = track();
    foreign.urn = "spotify:track:1".into();
    assert!(validate(&foreign).is_err());
}

#[test]
fn odd_numbers_and_tokens_are_dropped() {
    let mut embed = track();
    embed.duration_ms = Some(30 * 24 * 3_600_000);
    embed.at_ms = Some(10 * 24 * 3_600_000);
    embed.access = Some("free".into());
    embed.secret_token = Some("a/b".into());
    let clean = validate(&embed).unwrap();
    assert_eq!((clean.duration_ms, clean.at_ms), (None, None));
    assert_eq!((clean.access, clean.secret_token), (None, None));
    let mut late = track();
    late.at_ms = Some(300_000);
    assert_eq!(validate(&late).unwrap().at_ms, None);
}

#[test]
fn playlists_keep_five_tracks_and_others_none() {
    let mut playlist = track();
    playlist.kind = EmbedKind::Playlist;
    playlist.urn = "soundcloud:playlists:9".into();
    playlist.tracks = (0..8).map(listed).collect();
    playlist.tracks[1].urn = "bad".into();
    let clean = validate(&playlist).unwrap();
    assert_eq!(clean.tracks.len(), TRACKS_MAX);
    assert_eq!(clean.tracks[1].urn, "soundcloud:tracks:2");
    let mut single = track();
    single.tracks = vec![listed(1)];
    assert!(validate(&single).unwrap().tracks.is_empty());
}

#[test]
fn oversized_snapshots_shed_tracks_then_fail() {
    let mut playlist = track();
    playlist.kind = EmbedKind::Playlist;
    playlist.urn = "soundcloud:playlists:9".into();
    playlist.tracks = (0..5)
        .map(|n| {
            let mut big = listed(n);
            big.title = "Ж".repeat(120);
            big.artist = "Ж".repeat(120);
            big.artwork_url = Some(format!("https://i1.sndcdn.com/{}.jpg", "a".repeat(400)));
            big
        })
        .collect();
    let clean = validate(&playlist).unwrap();
    assert!(clean.tracks.len() < 5);
    assert!(serde_json::to_vec(&clean).unwrap().len() <= EMBED_BYTES_MAX);
}

#[test]
fn room_invites_need_a_real_code() {
    let invite = Embed {
        kind: EmbedKind::RoomInvite,
        urn: String::new(),
        permalink: String::new(),
        secret_token: None,
        title: String::new(),
        artist: None,
        artwork_url: None,
        duration_ms: None,
        access: None,
        at_ms: None,
        tracks: Vec::new(),
        room: Some(RoomInvite {
            code: "ab c-234".into(),
            title: Some("Night".into()),
            host: "Host".into(),
        }),
    };
    let clean = validate(&invite).unwrap();
    assert_eq!(clean.room.unwrap().code, "ABC234");
    let mut wrong = invite.clone();
    wrong.room.as_mut().unwrap().code = "ABC10O".into();
    assert!(validate(&wrong).is_err());
    let mut missing = invite;
    missing.room = None;
    assert!(validate(&missing).is_err());
    let mut stray = track();
    stray.room = Some(RoomInvite {
        code: "ABCDEF".into(),
        title: None,
        host: "h".into(),
    });
    assert_eq!(validate(&stray).unwrap().room, None);
}

#[test]
fn a_message_carries_at_most_three_cards() {
    let mut bad = track();
    bad.title = String::new();
    let embeds = vec![bad, track(), track(), track(), track()];
    assert_eq!(validate_all(&embeds).len(), 3);
}

#[test]
fn received_envelopes_are_sanitized_in_place() {
    let mut evil = track();
    evil.artwork_url = Some("https://evil.com/x.jpg".into());
    let mut user = track();
    user.kind = EmbedKind::User;
    user.urn = "soundcloud:users:5".into();
    let mut envelope = Envelope {
        v: 1,
        body: Some(Body::Suggest(Suggest {
            track: Some(Box::new(to_proto(&user))),
        })),
        embeds: vec![
            to_proto(&evil),
            proto::Embed::default(),
            to_proto(&track()),
            to_proto(&track()),
            to_proto(&track()),
        ],
        ..Envelope::default()
    };
    sanitize(&mut envelope);
    assert_eq!(envelope.embeds.len(), 3);
    assert_eq!(from_proto(&envelope.embeds[0]).unwrap().artwork_url, None);
    let Some(Body::Suggest(suggest)) = envelope.body else {
        panic!("suggest body lost");
    };
    assert!(suggest.track.is_none());
}

#[test]
fn proto_conversion_round_trips() {
    let mut playlist = track();
    playlist.kind = EmbedKind::Playlist;
    playlist.tracks = vec![listed(1), listed(2)];
    assert_eq!(from_proto(&to_proto(&playlist)), Some(playlist));
}
