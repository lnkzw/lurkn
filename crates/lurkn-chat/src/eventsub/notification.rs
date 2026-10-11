//! `channel.chat.notification` -> [`ChatMessage`].
//!
//! The channel events that show up in chat: subs, resubs, gifts, raids,
//! announcements, charity donations. Twitch delivers them on the same socket as
//! chat and under the same `user:read:chat` scope, which is what makes them
//! reachable at all — the dedicated topics (`channel.subscribe`, `channel.cheer`,
//! `channel.hype_train.*`) need the broadcaster's own token, so a viewer client
//! can never subscribe to them. See `docs/architecture.md` §5.2.
//!
//! A notification becomes an ordinary [`ChatMessage`] carrying a
//! [`ChannelEvent`], rather than a parallel kind of thing. Everything downstream
//! — badge resolution, emote resolution, batching, trimming, the user card —
//! then works on it unchanged.

use lurkn_core::chat::{fallback_color, ChatAuthor, ChatMessage, MessageState, Rgb};
use lurkn_core::{ChannelEvent, ChannelEventKind, Login, MessageId, MessageKind, UserId};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::eventsub::chat::{tokenize, BadgeRef, MessageBody};

#[derive(Debug, Deserialize)]
pub struct NotificationEvent {
    pub broadcaster_user_id: String,
    #[serde(default)]
    pub chatter_user_id: String,
    #[serde(default)]
    pub chatter_user_login: String,
    #[serde(default)]
    pub chatter_user_name: String,
    /// Gift subs can be given anonymously, and then the chatter fields are blank.
    #[serde(default)]
    pub chatter_is_anonymous: bool,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub badges: Vec<BadgeRef>,
    pub message_id: String,
    /// Twitch's own rendering of the event. The line Lurkn shows.
    #[serde(default)]
    pub system_message: String,
    pub notice_type: String,
    /// The chatter's own words, when the notice type allows them: a resub
    /// message, or the text of an announcement. Empty otherwise.
    #[serde(default)]
    pub message: Option<MessageBody>,

    #[serde(default)]
    pub resub: Option<Resub>,
    #[serde(default)]
    pub community_sub_gift: Option<CommunitySubGift>,
    #[serde(default)]
    pub raid: Option<Raid>,
    #[serde(default)]
    pub bits_badge_tier: Option<BitsBadgeTier>,
}

#[derive(Debug, Deserialize)]
pub struct Resub {
    #[serde(default)]
    pub cumulative_months: u32,
}

#[derive(Debug, Deserialize)]
pub struct CommunitySubGift {
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct Raid {
    #[serde(default)]
    pub viewer_count: u32,
}

#[derive(Debug, Deserialize)]
pub struct BitsBadgeTier {
    #[serde(default)]
    pub tier: u32,
}

/// The login shown for an anonymous gifter, which Twitch uses itself.
const ANONYMOUS: &str = "ananonymousgifter";

/// Turn a notification into a domain message.
///
/// Returns `None` when there is nothing to show: a notice type with no wording
/// from Twitch would render as a blank row, which reads as a bug.
pub fn to_message(event: NotificationEvent, received_at: OffsetDateTime) -> Option<ChatMessage> {
    if event.system_message.trim().is_empty() {
        return None;
    }

    // An anonymous gifter has no login of its own; Twitch's placeholder account
    // keeps the row shaped like every other one.
    let login = Login::parse(&event.chatter_user_login)
        .ok()
        .or_else(|| Login::parse(ANONYMOUS).ok())?;

    let kind = kind_of(&event.notice_type);
    let count = count_of(&event);

    let display_name = if event.chatter_is_anonymous || event.chatter_user_name.is_empty() {
        "Anonymous".to_owned()
    } else {
        event.chatter_user_name
    };

    let color = Rgb::parse(&event.color).unwrap_or_else(|| fallback_color(&login));
    let tokens = event.message.as_ref().map(tokenize).unwrap_or_default();
    let raw_text = event
        .message
        .as_ref()
        .map(|m| m.text.clone())
        .unwrap_or_default();

    Some(ChatMessage {
        id: MessageId(event.message_id),
        channel_id: UserId(event.broadcaster_user_id),
        author: ChatAuthor {
            id: UserId(event.chatter_user_id),
            login,
            display_name,
            color: Some(color),
            badges: event
                .badges
                .iter()
                .map(|b| {
                    lurkn_core::Badge::unresolved(b.set_id.clone(), b.id.clone(), b.info.clone())
                })
                .collect(),
        },
        tokens,
        raw_text,
        sent_at: received_at,
        // Highlighted rather than System: System is for lines Lurkn writes
        // itself, and these come from Twitch.
        kind: MessageKind::Highlight,
        state: MessageState::Sent,
        reply_to: None,
        event: Some(ChannelEvent {
            kind,
            text: event.system_message,
            count,
        }),
        // Decided by the pump, which is where the reader's own name and their
        // highlight words live; this crate knows neither.
        highlighted: false,
        highlight_color: None,
    })
}

/// Map Twitch's `notice_type` onto something the UI can style.
///
/// Unknown types fall through to [`ChannelEventKind::Other`] rather than being
/// dropped: Twitch adds notice types, and a plainly styled row carrying Twitch's
/// own wording is better than silence.
fn kind_of(notice_type: &str) -> ChannelEventKind {
    match notice_type {
        "sub" => ChannelEventKind::Sub,
        "resub" => ChannelEventKind::Resub,
        "sub_gift" => ChannelEventKind::SubGift,
        "community_sub_gift" => ChannelEventKind::CommunityGift,
        "gift_paid_upgrade" | "prime_paid_upgrade" | "pay_it_forward" => ChannelEventKind::Upgrade,
        "raid" => ChannelEventKind::Raid,
        "announcement" => ChannelEventKind::Announcement,
        "charity_donation" => ChannelEventKind::Charity,
        "bits_badge_tier" => ChannelEventKind::BitsBadge,
        _ => ChannelEventKind::Other,
    }
}

/// The one number worth emphasising, per kind. `None` where a count would only
/// repeat what Twitch's wording already says.
fn count_of(event: &NotificationEvent) -> Option<u32> {
    match event.notice_type.as_str() {
        "resub" => event.resub.as_ref().map(|r| r.cumulative_months),
        "community_sub_gift" => event.community_sub_gift.as_ref().map(|g| g.total),
        "raid" => event.raid.as_ref().map(|r| r.viewer_count),
        "bits_badge_tier" => event.bits_badge_tier.as_ref().map(|b| b.tier),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn parse(json: &str) -> Option<ChatMessage> {
        to_message(serde_json::from_str(json).expect("fixture parses"), NOW)
    }

    #[test]
    fn a_resub_carries_twitchs_wording_and_its_month_count() {
        let message = parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2","chatter_user_login":"forsen",
                "chatter_user_name":"Forsen","message_id":"M1","notice_type":"resub",
                "system_message":"Forsen subscribed at Tier 1. They've subscribed for 24 months!",
                "message":{"text":"still here","fragments":[{"type":"text","text":"still here"}]},
                "resub":{"cumulative_months":24,"duration_months":1}}"##,
        )
        .expect("a resub renders");

        let event = message.event.expect("a resub is a channel event");
        assert_eq!(event.kind, ChannelEventKind::Resub);
        assert_eq!(event.count, Some(24));
        assert!(event.text.contains("24 months"));
        // The subscriber's own words survive as an ordinary message body.
        assert_eq!(message.raw_text, "still here");
        assert_eq!(message.kind, MessageKind::Highlight);
    }

    #[test]
    fn a_raid_carries_the_viewer_count() {
        let message = parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2","chatter_user_login":"forsen",
                "chatter_user_name":"Forsen","message_id":"M2","notice_type":"raid",
                "system_message":"120 raiders from Forsen have joined!",
                "raid":{"user_id":"2","user_name":"Forsen","viewer_count":120}}"##,
        )
        .expect("a raid renders");

        let event = message.event.expect("a raid is a channel event");
        assert_eq!(event.kind, ChannelEventKind::Raid);
        assert_eq!(event.count, Some(120));
        assert!(message.tokens.is_empty(), "a raid has no message body");
    }

    #[test]
    fn a_community_gift_counts_the_subs_given() {
        let message = parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2","chatter_user_login":"forsen",
                "chatter_user_name":"Forsen","message_id":"M3","notice_type":"community_sub_gift",
                "system_message":"Forsen is gifting 5 Tier 1 Subs to the community!",
                "community_sub_gift":{"id":"G","total":5,"sub_tier":"1000"}}"##,
        )
        .expect("a community gift renders");

        let event = message.event.expect("a gift is a channel event");
        assert_eq!(event.kind, ChannelEventKind::CommunityGift);
        assert_eq!(event.count, Some(5));
    }

    /// Twitch sends no login for an anonymous gifter; the row must still render.
    #[test]
    fn an_anonymous_gift_still_renders() {
        let message = parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"","chatter_user_login":"",
                "chatter_user_name":"","chatter_is_anonymous":true,"message_id":"M4",
                "notice_type":"sub_gift",
                "system_message":"An anonymous user gifted a Tier 1 sub to lurker!",
                "sub_gift":{"duration_months":1,"recipient_user_name":"lurker"}}"##,
        )
        .expect("an anonymous gift renders");

        assert_eq!(message.author.display_name, "Anonymous");
        assert_eq!(message.author.login.as_str(), ANONYMOUS);
    }

    /// A notice type added after this build shipped still shows Twitch's words.
    #[test]
    fn an_unknown_notice_type_falls_back_rather_than_vanishing() {
        let message = parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2","chatter_user_login":"forsen",
                "chatter_user_name":"Forsen","message_id":"M5","notice_type":"something_new",
                "system_message":"Forsen did something new!"}"##,
        )
        .expect("an unknown notice renders");

        let event = message.event.expect("still a channel event");
        assert_eq!(event.kind, ChannelEventKind::Other);
        assert_eq!(event.text, "Forsen did something new!");
    }

    #[test]
    fn a_notice_with_no_wording_is_skipped() {
        assert!(parse(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2","chatter_user_login":"forsen",
                "chatter_user_name":"Forsen","message_id":"M6","notice_type":"sub",
                "system_message":"   "}"##,
        )
        .is_none());
    }
}
