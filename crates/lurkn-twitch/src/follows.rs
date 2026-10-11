use std::collections::HashMap;

use lurkn_core::{Category, FollowedChannel, Stream, TwitchChannel, User, UserId};

use crate::client::HelixClient;
use crate::dto::{ChannelInfoDto, FollowedChannelDto, StreamDto, UserDto};
use crate::error::Result;
use crate::{endpoints, MAX_PAGE_SIZE};

/// Guard against a runaway cursor: 100 pages is 10,000 follows.
const MAX_PAGES: usize = 100;
/// Helix batch endpoints take at most 100 ids per call.
const BATCH: usize = 100;

/// Reads the follow list and its live state. See `docs/architecture.md` §4.3.
#[derive(Debug, Clone)]
pub struct FollowsService {
    client: HelixClient,
}

impl FollowsService {
    pub fn new(client: HelixClient) -> Self {
        Self { client }
    }

    /// The account the token belongs to.
    pub async fn current_user(&self) -> Result<Option<User>> {
        let page: crate::client::Page<UserDto> =
            self.client.get(&endpoints::current_user()).await?;
        Ok(page.data.into_iter().next().and_then(UserDto::into_user))
    }

    /// Every followed channel, paginated. Follows change rarely, so this runs on
    /// a slow cadence rather than with the live poll.
    pub async fn followed_channels(&self, user_id: &str) -> Result<Vec<TwitchChannel>> {
        let dtos: Vec<FollowedChannelDto> = self
            .client
            .get_all(
                |cursor| endpoints::followed_channels_page(user_id, MAX_PAGE_SIZE, cursor),
                MAX_PAGES,
            )
            .await?;

        Ok(dtos
            .into_iter()
            .filter_map(FollowedChannelDto::into_channel)
            .collect())
    }

    /// The currently live subset.
    ///
    /// This is the whole live check: Helix returns only live channels here, so
    /// there is no id-batching step.
    pub async fn live_streams(&self, user_id: &str) -> Result<Vec<Stream>> {
        let dtos: Vec<StreamDto> = self
            .client
            .get_all(
                |cursor| endpoints::followed_streams_page(user_id, MAX_PAGE_SIZE, cursor),
                MAX_PAGES,
            )
            .await?;

        Ok(dtos.iter().filter_map(StreamDto::into_stream).collect())
    }

    /// Avatars and display names, 100 ids per request.
    pub async fn users(&self, ids: &[UserId]) -> Result<HashMap<UserId, User>> {
        let mut out = HashMap::with_capacity(ids.len());

        for chunk in ids.chunks(BATCH) {
            let raw: Vec<String> = chunk.iter().map(|id| id.0.clone()).collect();
            let page: crate::client::Page<UserDto> =
                self.client.get(&endpoints::users_by_id(&raw)).await?;
            for user in page.data.into_iter().filter_map(UserDto::into_user) {
                out.insert(user.id.clone(), user);
            }
        }

        Ok(out)
    }

    /// Live channels in the categories and languages the reader's follows use,
    /// minus the ones they already follow.
    pub async fn suggested(
        &self,
        follows: &[FollowedChannel],
        limit: usize,
    ) -> Result<Vec<lurkn_core::ChannelHit>> {
        let basis = crate::suggest::categories(follows);
        if basis.is_empty() {
            return Ok(Vec::new());
        }
        let languages = crate::suggest::languages(follows);
        let game_ids: Vec<String> = basis.iter().map(|b| b.id.clone()).collect();

        // Over-asked, because the reader's own follows are in these categories
        // too and come back in the answer, and are then dropped.
        const ASK: u32 = 60;
        let page: crate::client::Page<StreamDto> = self
            .client
            .get(&endpoints::streams_in(&game_ids, &languages, ASK))
            .await?;

        let followed: std::collections::HashSet<&str> =
            follows.iter().map(|f| f.channel.user.id.as_str()).collect();

        // Twitch already answers busiest first; kept as it comes.
        let streams: Vec<StreamDto> = page
            .data
            .into_iter()
            .filter(|s| !followed.contains(s.user_id.as_str()))
            .take(limit)
            .collect();

        if streams.is_empty() {
            return Ok(Vec::new());
        }

        // Get Streams carries no profile image, and a list of initials is much
        // harder to scan than a list of faces. One extra request for all of them.
        let ids: Vec<UserId> = streams.iter().map(|s| UserId(s.user_id.clone())).collect();
        let users = self.users(&ids).await.unwrap_or_default();

        Ok(streams
            .into_iter()
            .filter_map(|s| {
                let login = lurkn_core::Login::parse(&s.user_login).ok()?;
                let avatar_url = users
                    .get(&UserId(s.user_id.clone()))
                    .and_then(|u| u.avatar_url.clone());

                Some(lurkn_core::ChannelHit {
                    id: UserId(s.user_id),
                    login,
                    display_name: s.user_name,
                    avatar_url,
                    live: true,
                    category: (!s.game_name.is_empty()).then_some(s.game_name),
                    title: (!s.title.is_empty()).then_some(s.title),
                    viewers: Some(s.viewer_count),
                })
            })
            .collect())
    }

    /// Channels whose name matches `query`, live ones first.
    ///
    /// Twitch already puts live channels ahead of offline ones; the order is
    /// kept rather than re-sorted, because within each group it is ranking by
    /// relevance that this crate has no better way to judge.
    pub async fn search_channels(&self, query: &str) -> Result<Vec<lurkn_core::ChannelHit>> {
        // Enough to find someone by a partial name, few enough to scan.
        const LIMIT: u32 = 12;

        let page: crate::client::Page<crate::dto::SearchChannelDto> = self
            .client
            .get(&endpoints::search_channels(query, LIMIT))
            .await?;

        Ok(page
            .data
            .into_iter()
            .filter_map(crate::dto::SearchChannelDto::into_hit)
            .collect())
    }

    /// A channel the reader does not follow, in the same shape and with the
    /// same data as one they do.
    ///
    /// Built by [`assemble`], the function the follow list goes through, so the
    /// title, category, language and live state come from the same requests
    /// and the same merge rules. Anything else would drift: a channel opened by
    /// name would quietly lack whatever a follow has, which is exactly how it
    /// ended up with no title and no `!commands` before this existed.
    pub async fn channel_state(&self, user: User) -> Result<FollowedChannel> {
        let id = user.id.clone();

        let info = self.channel_info(std::slice::from_ref(&id)).await?;
        let page: crate::client::Page<StreamDto> = self
            .client
            .get(&endpoints::streams_by_user(std::slice::from_ref(&id.0)))
            .await?;
        let live: Vec<Stream> = page
            .data
            .iter()
            .filter_map(StreamDto::into_stream)
            .collect();

        let mut users = HashMap::new();
        users.insert(id.clone(), user.clone());

        let channel = TwitchChannel {
            user,
            title: None,
            category: None,
            language: None,
            followed_at: None,
        };

        assemble(vec![channel], live, &users, &info)
            .pop()
            .ok_or_else(|| {
                crate::error::HelixError::Malformed(
                    "assembling one channel produced none".to_owned(),
                )
            })
    }

    /// Find an account by the name it is known by.
    ///
    /// `None` means Twitch has no such login — a typo, or an account that has
    /// been renamed or closed — which is a different thing from a failed
    /// request and is reported as such.
    pub async fn user_by_login(&self, login: &lurkn_core::Login) -> Result<Option<User>> {
        let page: crate::client::Page<UserDto> = self
            .client
            .get(&endpoints::users_by_login(&[login.to_string()]))
            .await?;

        Ok(page.data.into_iter().next().and_then(UserDto::into_user))
    }

    /// Everything `Get Users` knows about one account, for the user card.
    ///
    /// One id rather than a batch: a card is opened for the one name the reader
    /// clicked, and the caller caches the answer.
    pub async fn profile(&self, id: &UserId) -> Result<Option<lurkn_core::UserProfile>> {
        let page: crate::client::Page<UserDto> = self
            .client
            .get(&endpoints::users_by_id(std::slice::from_ref(&id.0)))
            .await?;

        Ok(page.data.into_iter().next().and_then(UserDto::into_profile))
    }

    /// Title and category for channels that are not live.
    pub async fn channel_info(&self, ids: &[UserId]) -> Result<HashMap<UserId, ChannelInfoDto>> {
        let mut out = HashMap::with_capacity(ids.len());

        for chunk in ids.chunks(BATCH) {
            let raw: Vec<String> = chunk.iter().map(|id| id.0.clone()).collect();
            let page: crate::client::Page<ChannelInfoDto> =
                self.client.get(&endpoints::channels_by_id(&raw)).await?;
            for info in page.data {
                out.insert(UserId(info.broadcaster_id.clone()), info);
            }
        }

        Ok(out)
    }
}

/// Join a follow list with live streams and freshly fetched metadata.
///
/// Kept as a free function over plain data so the ordering rules below are
/// testable without any network.
pub fn assemble(
    channels: Vec<TwitchChannel>,
    live: Vec<Stream>,
    users: &HashMap<UserId, User>,
    info: &HashMap<UserId, ChannelInfoDto>,
) -> Vec<FollowedChannel> {
    let mut by_channel: HashMap<UserId, Stream> = live
        .into_iter()
        .map(|s| (s.channel_id.clone(), s))
        .collect();

    let mut out: Vec<FollowedChannel> = channels
        .into_iter()
        .map(|mut channel| {
            let id = channel.user.id.clone();

            if let Some(user) = users.get(&id) {
                // Helix's follow list omits the avatar, so fill it from Get Users.
                channel.user.avatar_url = user.avatar_url.clone();
                channel.user.display_name = user.display_name.clone();
            }

            let stream = by_channel.remove(&id);

            match (&stream, info.get(&id)) {
                // A live stream is more current than the channel record.
                (Some(s), _) => {
                    channel.title = Some(s.title.clone());
                    channel.category = s.category.clone();
                }
                (None, Some(i)) => {
                    channel.title = (!i.title.is_empty()).then(|| i.title.clone());
                    channel.category = (!i.game_id.is_empty()).then(|| Category {
                        id: i.game_id.clone(),
                        name: i.game_name.clone(),
                        box_art_url: None,
                    });
                    channel.language = (!i.broadcaster_language.is_empty())
                        .then(|| i.broadcaster_language.clone());
                }
                (None, None) => {}
            }

            FollowedChannel { channel, stream }
        })
        .collect();

    sort_for_sidebar(&mut out);
    out
}

/// Live first by viewer count, then offline alphabetically.
pub fn sort_for_sidebar(list: &mut [FollowedChannel]) {
    list.sort_by(|a, b| match (&a.stream, &b.stream) {
        (Some(x), Some(y)) => y
            .viewer_count
            .cmp(&x.viewer_count)
            .then_with(|| a.channel.user.login.cmp(&b.channel.user.login)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a
            .channel
            .user
            .display_name
            .to_lowercase()
            .cmp(&b.channel.user.display_name.to_lowercase()),
    });
}

/// What changed between two live sets, so the UI can be told only the delta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveDelta {
    pub went_live: Vec<UserId>,
    pub went_offline: Vec<UserId>,
}

impl LiveDelta {
    pub fn is_empty(&self) -> bool {
        self.went_live.is_empty() && self.went_offline.is_empty()
    }
}

pub fn diff_live(previous: &[UserId], current: &[UserId]) -> LiveDelta {
    let before: std::collections::HashSet<&UserId> = previous.iter().collect();
    let after: std::collections::HashSet<&UserId> = current.iter().collect();

    LiveDelta {
        went_live: current
            .iter()
            .filter(|id| !before.contains(id))
            .cloned()
            .collect(),
        went_offline: previous
            .iter()
            .filter(|id| !after.contains(id))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use lurkn_core::Login;

    use super::*;

    fn channel(id: &str, login: &str, name: &str) -> TwitchChannel {
        TwitchChannel {
            user: User {
                id: UserId(id.into()),
                login: Login::parse(login).unwrap(),
                display_name: name.into(),
                avatar_url: None,
            },
            title: None,
            category: None,
            language: None,
            followed_at: None,
        }
    }

    fn stream(id: &str, viewers: u32, title: &str) -> Stream {
        Stream {
            channel_id: UserId(id.into()),
            title: title.into(),
            category: Some(Category {
                id: "1".into(),
                name: "Just Chatting".into(),
                box_art_url: None,
            }),
            viewer_count: viewers,
            started_at: time::macros::datetime!(2026-09-20 12:00:00 UTC),
            thumbnail_url_template: None,
        }
    }

    #[test]
    fn live_channels_sort_above_offline_ones_by_viewers() {
        let out = assemble(
            vec![
                channel("1", "alpha", "Alpha"),
                channel("2", "bravo", "Bravo"),
                channel("3", "charlie", "Charlie"),
                channel("4", "delta", "Delta"),
            ],
            vec![stream("3", 50, "small"), stream("2", 9000, "big")],
            &HashMap::new(),
            &HashMap::new(),
        );

        let order: Vec<&str> = out.iter().map(|f| f.channel.user.login.as_str()).collect();
        assert_eq!(order, ["bravo", "charlie", "alpha", "delta"]);
        assert!(out[0].is_live() && out[1].is_live());
        assert!(!out[2].is_live() && !out[3].is_live());
    }

    #[test]
    fn offline_channels_sort_case_insensitively_by_display_name() {
        let out = assemble(
            vec![
                channel("1", "zed", "zed"),
                channel("2", "apple", "Apple"),
                channel("3", "beta", "BETA"),
            ],
            vec![],
            &HashMap::new(),
            &HashMap::new(),
        );
        let order: Vec<&str> = out
            .iter()
            .map(|f| f.channel.user.display_name.as_str())
            .collect();
        assert_eq!(order, ["Apple", "BETA", "zed"]);
    }

    #[test]
    fn a_live_stream_supplies_the_title_and_category() {
        let out = assemble(
            vec![channel("1", "alpha", "Alpha")],
            vec![stream("1", 5, "streaming now")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(out[0].channel.title.as_deref(), Some("streaming now"));
        assert_eq!(
            out[0].channel.category.as_ref().unwrap().name,
            "Just Chatting"
        );
    }

    #[test]
    fn an_offline_channel_falls_back_to_its_channel_record() {
        let mut info = HashMap::new();
        info.insert(
            UserId("1".into()),
            ChannelInfoDto {
                broadcaster_id: "1".into(),
                broadcaster_language: "de".into(),
                game_id: "77".into(),
                game_name: "Path of Exile".into(),
                title: "last stream title".into(),
            },
        );

        let out = assemble(
            vec![channel("1", "alpha", "Alpha")],
            vec![],
            &HashMap::new(),
            &info,
        );
        assert_eq!(out[0].channel.title.as_deref(), Some("last stream title"));
        assert_eq!(
            out[0].channel.category.as_ref().unwrap().name,
            "Path of Exile"
        );
        assert_eq!(out[0].channel.language.as_deref(), Some("de"));
    }

    #[test]
    fn avatars_come_from_get_users_since_the_follow_list_omits_them() {
        let mut users = HashMap::new();
        users.insert(
            UserId("1".into()),
            User {
                id: UserId("1".into()),
                login: Login::parse("alpha").unwrap(),
                display_name: "AlphaPrime".into(),
                avatar_url: url::Url::parse("https://static-cdn.jtvnw.net/a.png").ok(),
            },
        );

        let out = assemble(
            vec![channel("1", "alpha", "Alpha")],
            vec![],
            &users,
            &HashMap::new(),
        );
        assert_eq!(out[0].channel.user.display_name, "AlphaPrime");
        assert!(out[0].channel.user.avatar_url.is_some());
    }

    /// A stream for someone no longer followed must not invent a sidebar row.
    #[test]
    fn a_stream_without_a_matching_follow_is_dropped() {
        let out = assemble(
            vec![channel("1", "alpha", "Alpha")],
            vec![stream("999", 10, "ghost")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(out.len(), 1);
        assert!(!out[0].is_live());
    }

    #[test]
    fn diffing_reports_only_what_changed() {
        let before = vec![UserId("1".into()), UserId("2".into())];
        let after = vec![UserId("2".into()), UserId("3".into())];

        let delta = diff_live(&before, &after);
        assert_eq!(delta.went_live, vec![UserId("3".into())]);
        assert_eq!(delta.went_offline, vec![UserId("1".into())]);
        assert!(!delta.is_empty());
    }

    #[test]
    fn an_unchanged_live_set_produces_no_delta() {
        let set = vec![UserId("1".into()), UserId("2".into())];
        assert!(diff_live(&set, &set).is_empty());
        assert!(diff_live(&[], &[]).is_empty());
    }
}
