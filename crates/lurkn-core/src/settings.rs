//! User settings. The defaults must be good enough that nobody
//! has to open this screen.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::chat::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum EmoteSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl EmoteSize {
    /// Rendered height as a multiple of the line's font size.
    pub fn scale(self) -> f32 {
        match self {
            Self::Small => 1.35,
            Self::Medium => 1.75,
            Self::Large => 2.25,
        }
    }
}

/// Font used for chat messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum ChatFont {
    /// Whatever the rest of the UI uses.
    #[default]
    System,
    Sans,
    Serif,
    Monospace,
    /// Use `custom_font`, if it survives validation.
    Custom,
}

/// The longest custom family name accepted. Real names are far shorter; this is
/// a bound on what a corrupt settings file can push into a stylesheet.
pub const MAX_FONT_NAME: usize = 64;

/// Keep only what can appear in a font family name.
///
/// The value ends up in a CSS `font-family`, so anything that could close the
/// declaration and start another — quotes, semicolons, braces, parentheses — is
/// dropped rather than escaped. A name that is empty afterwards is refused, and
/// the caller falls back to the system font.
pub fn sanitize_font_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();

    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(MAX_FONT_NAME).collect())
}

/// Where the chat status pills sit relative to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum StatusBarPosition {
    /// Directly under the chat header.
    Top,
    /// Directly above the message box.
    #[default]
    Bottom,
}

/// Which edge the followed-channels drawer slides in from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum SidebarPosition {
    #[default]
    Left,
    Right,
    Top,
}

impl SidebarPosition {
    /// Whether the drawer is a vertical column. `Top` is a horizontal strip, so
    /// the collapsed layout differs.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SidebarSettings {
    pub position: SidebarPosition,
    /// Avatars only. In this mode the list is docked rather than a drawer, so it
    /// stays on screen as a quick channel switcher.
    pub collapsed: bool,
    /// Whether channels that are not live appear in the list at all.
    pub show_offline: bool,
}

impl Default for SidebarSettings {
    fn default() -> Self {
        Self {
            position: SidebarPosition::default(),
            collapsed: false,
            show_offline: true,
        }
    }
}

impl SidebarSettings {
    /// Collapsed mode is a permanent rail; expanded mode is an on-demand drawer.
    pub fn is_docked(&self) -> bool {
        self.collapsed
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowSettings {
    /// Hide Lurkn's own title bar. The window stays undecorated either way, so
    /// the chat header becomes the drag handle when this is on.
    pub hide_title_bar: bool,
    /// Draw scrollbars in the chat log and channel list. Scrolling by wheel or
    /// keyboard works either way.
    pub show_scrollbars: bool,
    /// Show the bar above the chat: channel name, stream title, the watch
    /// button and the menu. Hidden, the menu is a right-click on the chat.
    pub show_header: bool,
    /// Pixels, for the chat header and the title bar. Clamped like the chat's.
    pub header_font_size: u8,
    pub header_font: ChatFont,
    /// Only consulted when `header_font` is [`ChatFont::Custom`].
    pub header_custom_font: String,
    /// Open settings in a window of their own rather than over the chat, which
    /// a narrow Lurkn has little room for.
    pub settings_in_window: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            hide_title_bar: false,
            show_scrollbars: true,
            show_header: true,
            header_font_size: 15,
            header_font: ChatFont::default(),
            header_custom_font: String::new(),
            settings_in_window: false,
        }
    }
}

pub const MIN_HEADER_FONT_SIZE: u8 = 11;
/// The bar is one avatar row high; past this the name no longer fits it.
pub const MAX_HEADER_FONT_SIZE: u8 = 20;

/// How the browser should present a stream.
///
/// Not every browser can do every mode; what a given one supports is decided
/// where it is launched, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum LaunchMode {
    /// An ordinary tab or window, with the browser's normal furniture.
    #[default]
    Normal,
    /// One window holding just the page: no tabs, no address bar. Chromium's
    /// `--app=`; Firefox has no equivalent.
    App,
    /// Fullscreen with no browser chrome at all.
    Kiosk,
}

/// Which Twitch page a stream opens on.
///
/// Twitch has no URL that turns theatre mode on: it is a setting the site keeps
/// per account and applies itself. The player page is what a URL *can* ask for,
/// and it is the same thing theatre mode is wanted for — the video, large, with
/// nothing else around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum StreamPage {
    /// The full channel page: video, Twitch's own chat, panels.
    #[default]
    Channel,
    /// The bare player, video only. Lurkn already has the chat.
    Player,
}

/// The longest custom command accepted, as a bound on what a corrupt settings
/// file can hand to the process spawner.
pub const MAX_COMMAND_LINE: usize = 512;

/// What opens a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BrowserChoice {
    /// Whatever the desktop says handles https. The default, and the fallback
    /// whenever a more specific choice cannot be honoured.
    #[default]
    System,
    /// A browser Lurkn found installed, named by its desktop-entry id.
    Installed { id: String },
    /// Any program at all, as a command line. `%u` marks where the URL goes;
    /// without it the URL is appended.
    Command { line: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BrowserSettings {
    pub choice: BrowserChoice,
    pub mode: LaunchMode,
    /// Force a new window rather than a tab in one that is already open.
    /// Only meaningful for [`LaunchMode::Normal`]: app and kiosk always get a
    /// window of their own.
    pub new_window: bool,
    pub page: StreamPage,
    /// Experimental: keep one browser window on a page Lurkn serves, and switch
    /// the stream inside it instead of opening another tab.
    ///
    /// Off by default, and everything else keeps working when it is off — see
    /// `docs/architecture.md` §7.2 for why this exists and what it costs.
    pub sync_window: bool,
    /// With [`Self::sync_window`] on, changing chat also changes the stream in
    /// the window that is already open.
    ///
    /// Only ever *switches* a window; it never opens one, or arrow-keying down
    /// the sidebar would launch a browser. See `docs/architecture.md` §8.3.
    pub follow_chat: bool,
}

/// Accept a settings file from before this was a group.
///
/// The field used to hold the string `"systemDefault"`. Serde's `default` only
/// covers a *missing* field, so without this an older file fails to parse and
/// the user silently loses every other setting too.
fn browser_settings_compat<'de, D>(deserializer: D) -> Result<BrowserSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Current(BrowserSettings),
        /// Anything else at all, including the old string.
        Legacy(serde::de::IgnoredAny),
    }

    Ok(match Stored::deserialize(deserializer)? {
        Stored::Current(settings) => settings,
        Stored::Legacy(_) => BrowserSettings::default(),
    })
}

/// The activity feed: which of a channel's events are worth a column of their
/// own beside the chat.
///
/// Every one of these already arrives over the chat socket, so the feed is a
/// filtered view of the log rather than a second source. Follows and hype
/// trains are the two that do not, and both need scopes the reader has to
/// grant; they are absent until then rather than shown as empty rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivitySettings {
    /// Show the feed at all.
    pub enabled: bool,
    pub subs: bool,
    pub gifts: bool,
    pub raids: bool,
    pub bits: bool,
    pub announcements: bool,
    /// Anything a moderator says, not moderation actions.
    pub moderators: bool,
    /// Show the feed in a window of its own rather than as a column beside
    /// the chat.
    pub in_window: bool,
}

impl Default for ActivitySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            in_window: false,
            subs: true,
            gifts: true,
            raids: true,
            bits: true,
            announcements: true,
            // Off: in a busy channel the mods talk as much as anyone, and a
            // feed full of ordinary conversation is a second chat log.
            moderators: false,
        }
    }
}

/// Whether Lurkn looks for a newer version of itself.
///
/// The check is one request for a small file on GitHub, where releases are
/// published; nothing is installed without the reader asking for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateSettings {
    /// Look on start and twice a day after that.
    pub check: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        // On: an outdated chat client breaks quietly when Twitch changes
        // something, and the check says nothing about the reader.
        Self { check: true }
    }
}

/// What Lurkn does while the reader is the one streaming.
///
/// Whether they are live comes from Twitch's `stream.online`, which needs no
/// scope. Lurkn deliberately does not look for a running OBS or a screen share:
/// both say an application is open, not that Twitch has a stream, and they are
/// wrong in exactly the cases that matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct BroadcastSettings {
    /// Switch chat to your own channel when your stream starts.
    pub follow_own_stream: bool,
    /// Say so when your stream starts and stops.
    pub notify_own_stream: bool,
}

impl Default for BroadcastSettings {
    fn default() -> Self {
        Self {
            // Off: someone reading another channel when they go live did not
            // necessarily ask to be moved out of it.
            follow_own_stream: false,
            notify_own_stream: true,
        }
    }
}

/// How the stream overlay lays a message out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum OverlayLayout {
    /// Each message in a rounded box of its own, the name above the text.
    #[default]
    Bubbles,
    /// One line per message, name then text, straight on the scene.
    Plain,
}

/// Whose chat the stream overlay shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum OverlaySource {
    /// The reader's own channel, whatever is open in Lurkn.
    #[default]
    Own,
    /// Whichever channel is open in Lurkn, for watch-alongs and co-streams.
    /// It follows every switch — including a click on someone the reader is
    /// about to raid — which is why it is not the default.
    Open,
}

/// The port the overlay listens on unless the reader picks another.
///
/// Fixed rather than ephemeral, unlike the watch window's: OBS saves the URL in
/// the scene, and a port that changed on every start would break it every time.
/// Below Linux's ephemeral range, so an outgoing connection never holds it.
pub const DEFAULT_OVERLAY_PORT: u16 = 7373;
pub const MIN_OVERLAY_FONT_SIZE: u8 = 12;
pub const MAX_OVERLAY_FONT_SIZE: u8 = 64;
pub const MAX_OVERLAY_RADIUS: u8 = 32;
pub const MAX_OVERLAY_MESSAGES: u8 = 50;
/// Ten minutes. Past that, "fade out" and "never" look the same on a stream.
pub const MAX_OVERLAY_FADE: u16 = 600;

/// A channel's chat, served over loopback for a browser source in OBS.
///
/// It shows chat and nothing else: subs, raids and the rest are the activity
/// feed's business and have overlays of their own in every streaming tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlaySettings {
    pub enabled: bool,
    pub port: u16,
    pub source: OverlaySource,
    pub layout: OverlayLayout,
    pub font: ChatFont,
    pub custom_font: String,
    pub font_size: u8,
    pub text_color: Rgb,
    pub bubble_color: Rgb,
    /// Percent. Bubbles only.
    pub bubble_opacity: u8,
    /// Pixels. Bubbles only.
    pub corner_radius: u8,
    /// Each chatter's own Twitch colour, or `name_color` for everyone.
    pub twitch_name_colors: bool,
    pub name_color: Rgb,
    /// A dark edge around the text, so it reads over a busy scene.
    pub outline: bool,
    pub show_badges: bool,
    pub emote_size: EmoteSize,
    pub max_messages: u8,
    /// Seconds a message stays before it fades. Zero keeps it until it is
    /// pushed out by newer ones.
    pub fade_after_seconds: u16,
    /// Slide new messages in rather than have them appear.
    pub animate: bool,
    /// Leave out `!commands`: they are addressed to a bot, not the audience.
    pub hide_commands: bool,
    /// Chatters left out of the overlay but not out of Lurkn, which is what a
    /// bot usually needs.
    pub hidden_users: Vec<String>,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_OVERLAY_PORT,
            source: OverlaySource::default(),
            layout: OverlayLayout::default(),
            font: ChatFont::default(),
            custom_font: String::new(),
            font_size: 20,
            text_color: Rgb {
                r: 0xef,
                g: 0xef,
                b: 0xf1,
            },
            bubble_color: Rgb {
                r: 0x18,
                g: 0x18,
                b: 0x1b,
            },
            bubble_opacity: 85,
            corner_radius: 12,
            twitch_name_colors: true,
            name_color: Rgb {
                r: 0xbf,
                g: 0x94,
                b: 0xff,
            },
            outline: true,
            show_badges: true,
            emote_size: EmoteSize::default(),
            max_messages: 12,
            fade_after_seconds: 0,
            animate: true,
            hide_commands: true,
            // The bots most channels run. Their replies are for the person who
            // asked, and on stream they read as noise.
            hidden_users: [
                "nightbot",
                "streamelements",
                "streamlabs",
                "moobot",
                "fossabot",
                "wizebot",
                "soundalerts",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

/// Desktop notifications, which are the only way Lurkn can say anything while
/// its window is hidden behind the tray.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationSettings {
    /// A followed channel starting a stream.
    pub live: bool,
    /// A whisper arriving.
    pub whispers: bool,
    /// A message naming you, or matching a highlight keyword.
    pub highlights: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            live: true,
            whispers: true,
            // Off by default: a busy channel can name you often, and a client
            // that starts by making noise is one people turn off entirely.
            highlights: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
// `default` matters on every nested group, not just the root: without it a
// settings file written before a field existed fails to parse, and the user
// silently loses every other setting too.
#[serde(rename_all = "camelCase", default)]
pub struct ChatSettings {
    /// Pixels. Clamped on the way in so a bad stored value cannot break the UI.
    pub font_size: u8,
    pub show_timestamps: bool,
    pub show_badges: bool,
    pub emote_size: EmoteSize,
    /// Show the room-mode and connection pills at all.
    pub show_status_bar: bool,
    pub status_bar_position: StatusBarPosition,
    pub font: ChatFont,
    /// Only consulted when `font` is [`ChatFont::Custom`].
    pub custom_font: String,
    /// Tint every other message so long lines are easier to follow.
    pub alternate_backgrounds: bool,
    /// Draw a line where the reader last stopped reading.
    pub show_unread_divider: bool,
    /// Offer the `!commands` advertised in the stream title from the composer.
    pub show_command_button: bool,
    /// Show the Send button. Enter still sends when it is hidden.
    pub show_send_button: bool,
    /// Show subs, gifts, raids and announcements in the log.
    pub show_channel_events: bool,
    /// Show what a reply is replying to, above the reply itself.
    pub show_reply_preview: bool,
    /// Show what `@`, `!` and `:` do, while the composer is empty.
    pub show_composer_hints: bool,
    /// Mark messages that name you.
    pub highlight_mentions: bool,
    /// Legacy: highlight words from before [`HighlightSettings`] existed. Read
    /// so an older file keeps its words, moved into
    /// [`HighlightSettings::phrases`] by [`Settings::sanitized`], and empty
    /// from then on.
    pub highlight_keywords: Vec<String>,
    /// Backfill a channel's recent chat on join, from a third-party service.
    ///
    /// Off by default, and deliberately: Twitch serves no history, so the only
    /// source is `recent-messages.robotty.de`, and turning this on tells that
    /// service which channels are being opened. That is the user's call.
    pub history_on_join: bool,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            font_size: 13,
            show_timestamps: true,
            show_badges: true,
            emote_size: EmoteSize::default(),
            show_status_bar: true,
            status_bar_position: StatusBarPosition::default(),
            font: ChatFont::default(),
            custom_font: String::new(),
            alternate_backgrounds: false,
            show_unread_divider: true,
            show_command_button: true,
            show_send_button: true,
            show_channel_events: true,
            show_reply_preview: true,
            show_composer_hints: true,
            highlight_mentions: true,
            highlight_keywords: Vec::new(),
            history_on_join: false,
        }
    }
}

/// The most rules of one kind kept. Far past what anyone maintains by hand; a
/// bound on what a corrupt settings file can make every message run through.
pub const MAX_RULES: usize = 200;
/// The longest pattern, filter or command template accepted.
pub const MAX_PATTERN: usize = 1000;

/// A phrase that marks a message.
///
/// Plain text matches as a whole word, case-insensitively — "ann" does not fire
/// on "announcement" — which is what people expect when they type a word. `regex` takes the pattern as written instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HighlightPhrase {
    pub pattern: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// The row's tint. `None` uses the accent, the same as being named.
    pub color: Option<Rgb>,
    /// Also raise a desktop notification.
    pub notify: bool,
}

/// Everything one chatter says is marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HighlightUser {
    /// Lowercased on the way in, like every login.
    pub login: String,
    pub color: Option<Rgb>,
    pub notify: bool,
}

/// Everything said by someone wearing a badge is marked: moderators, VIPs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HighlightBadge {
    /// Twitch's badge set id: `moderator`, `vip`, `subscriber`, …
    pub badge: String,
    pub color: Option<Rgb>,
    pub notify: bool,
}

/// What marks a message besides being named: the Highlights page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HighlightSettings {
    pub phrases: Vec<HighlightPhrase>,
    pub users: Vec<HighlightUser>,
    pub badges: Vec<HighlightBadge>,
    /// Never highlighted, whatever they say — the bot that repeats your name.
    pub excluded_users: Vec<String>,
}

/// What an ignore phrase does to a message it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum IgnoreAction {
    /// The message is never shown.
    #[default]
    Hide,
    /// The matched text is swapped for [`IgnorePhrase::replacement`], and the
    /// rest of the message stays.
    Replace,
}

/// A phrase that hides a message, or is cut out of it.
///
/// Plain text matches anywhere in the message, case-insensitively: an ignore
/// is usually aimed at a fragment — a spam link, a copypasta's opening — not a
/// word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct IgnorePhrase {
    pub pattern: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub action: IgnoreAction,
    /// What a [`IgnoreAction::Replace`] match becomes. With `regex`, `$1` and
    /// friends refer to the pattern's groups.
    pub replacement: String,
}

/// Messages and chatters never shown: the Ignores page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct IgnoreSettings {
    pub phrases: Vec<IgnorePhrase>,
    /// Logins whose messages are dropped before they reach the log.
    pub users: Vec<String>,
}

/// A named condition a message must meet to be shown.
///
/// The expression is parsed and evaluated in the frontend (`src/lib/chat/filter.ts`):
/// a filter is a view of the log, and switching one off has to bring back what
/// it hid, which only the side holding the log can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatFilter {
    pub name: String,
    pub expression: String,
    /// Applied to the chat. Several on at once must all match.
    pub enabled: bool,
}

/// A command of the reader's own, expanded in the composer before sending.
///
/// `/hi` with `Hello {1}, welcome to {channel.name}!` makes
/// `/hi forsen` send the filled-in line. What it expands to is handled like
/// anything typed, so it may itself be `/w`, `/timeout` or plain chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomCommand {
    /// The first word that runs it, `/hi` or `!hi`. One word, no spaces.
    pub trigger: String,
    pub template: String,
    /// Offer it on the right-click menu of a message, where `{user.name}` and
    /// `{msg.text}` refer to that message.
    pub in_menu: bool,
}

/// The longest command trigger kept.
pub const MAX_TRIGGER: usize = 32;
/// The longest filter name kept.
pub const MAX_FILTER_NAME: usize = 64;

/// A login as a rule names it: lowercased, without the `@`, and only the
/// characters Twitch allows. Empty when nothing usable is left.
pub fn normalize_login(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('@')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(25)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// A list of logins: normalised, de-duplicated, bounded, blanks dropped.
fn login_list(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for login in list.iter().map(|l| normalize_login(l)) {
        if !login.is_empty() && !out.contains(&login) {
            out.push(login);
        }
    }
    out.truncate(MAX_RULES);
    out
}

fn bounded(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

pub const MIN_FONT_SIZE: u8 = 10;
pub const MAX_FONT_SIZE: u8 = 24;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemePreference,
    /// A theme file's id (its stem in the themes folder), or empty for the
    /// built-in palette `theme` picks. A file wins over `theme` while it loads.
    pub custom_theme: String,
    pub chat: ChatSettings,
    pub notifications: NotificationSettings,
    pub broadcast: BroadcastSettings,
    pub activity: ActivitySettings,
    pub sidebar: SidebarSettings,
    pub window: WindowSettings,
    pub highlights: HighlightSettings,
    pub ignores: IgnoreSettings,
    pub filters: Vec<ChatFilter>,
    pub commands: Vec<CustomCommand>,
    // The compat shim reads an older file's value; the wire type is unchanged,
    // which specta cannot tell on its own.
    #[serde(deserialize_with = "browser_settings_compat")]
    #[specta(type = BrowserSettings)]
    pub browser: BrowserSettings,
    pub overlay: OverlaySettings,
    pub updates: UpdateSettings,
}

impl Settings {
    /// Bring a stored or user-supplied value back into range.
    ///
    /// Settings round-trip through JSON on disk, so a hand-edited or
    /// partially-written file must not be able to produce an unusable UI.
    pub fn sanitized(mut self) -> Self {
        self.custom_theme = crate::theme::sanitize_theme_id(&self.custom_theme).unwrap_or_default();
        self.chat.font_size = self.chat.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.chat.custom_font = sanitize_font_name(&self.chat.custom_font).unwrap_or_default();

        // A custom font with no usable name would render as nothing at all.
        if self.chat.font == ChatFont::Custom && self.chat.custom_font.is_empty() {
            self.chat.font = ChatFont::System;
        }

        let window = &mut self.window;
        window.header_font_size = window
            .header_font_size
            .clamp(MIN_HEADER_FONT_SIZE, MAX_HEADER_FONT_SIZE);
        window.header_custom_font =
            sanitize_font_name(&window.header_custom_font).unwrap_or_default();
        if window.header_font == ChatFont::Custom && window.header_custom_font.is_empty() {
            window.header_font = ChatFont::System;
        }

        self.browser.choice = match std::mem::take(&mut self.browser.choice) {
            // A blank choice cannot be launched; fall back rather than fail at
            // the moment the user clicks.
            BrowserChoice::Installed { id } if id.trim().is_empty() => BrowserChoice::System,
            BrowserChoice::Command { line } if line.trim().is_empty() => BrowserChoice::System,
            BrowserChoice::Command { line } => BrowserChoice::Command {
                line: line.trim().chars().take(MAX_COMMAND_LINE).collect(),
            },
            other => other,
        };

        self.sanitize_overlay();
        self.sanitize_rules();
        self
    }

    /// The overlay's values reach a stylesheet in OBS and a port bind, so both
    /// get the same bounds the chat's own font does.
    fn sanitize_overlay(&mut self) {
        let overlay = &mut self.overlay;
        overlay.port = overlay.port.max(1024);
        overlay.font_size = overlay
            .font_size
            .clamp(MIN_OVERLAY_FONT_SIZE, MAX_OVERLAY_FONT_SIZE);
        overlay.custom_font = sanitize_font_name(&overlay.custom_font).unwrap_or_default();
        if overlay.font == ChatFont::Custom && overlay.custom_font.is_empty() {
            overlay.font = ChatFont::System;
        }
        overlay.bubble_opacity = overlay.bubble_opacity.min(100);
        overlay.corner_radius = overlay.corner_radius.min(MAX_OVERLAY_RADIUS);
        overlay.max_messages = overlay.max_messages.clamp(1, MAX_OVERLAY_MESSAGES);
        overlay.fade_after_seconds = overlay.fade_after_seconds.min(MAX_OVERLAY_FADE);
        overlay.hidden_users = login_list(std::mem::take(&mut overlay.hidden_users));
    }

    /// Bound the rule lists, and move the old keyword list into phrases.
    ///
    /// Rows are kept even while empty: the settings screen adds a blank row for
    /// the reader to fill in, and the save that follows must not delete it. A
    /// blank pattern matches nothing, so keeping it costs nothing.
    fn sanitize_rules(&mut self) {
        // The words of an older file become phrases with the same behaviour:
        // whole words, any case, and a notification if notifications were on.
        for word in std::mem::take(&mut self.chat.highlight_keywords) {
            let word = word.trim();
            if word.is_empty() {
                continue;
            }
            self.highlights.phrases.push(HighlightPhrase {
                pattern: word.to_owned(),
                notify: self.notifications.highlights,
                ..Default::default()
            });
        }

        let highlights = &mut self.highlights;
        highlights.phrases.truncate(MAX_RULES);
        for phrase in &mut highlights.phrases {
            phrase.pattern = bounded(&phrase.pattern, MAX_PATTERN);
        }
        highlights.users.truncate(MAX_RULES);
        for user in &mut highlights.users {
            user.login = normalize_login(&user.login);
        }
        highlights.badges.truncate(MAX_RULES);
        for badge in &mut highlights.badges {
            badge.badge = bounded(badge.badge.trim(), 64).to_ascii_lowercase();
        }
        highlights.excluded_users = login_list(std::mem::take(&mut highlights.excluded_users));

        let ignores = &mut self.ignores;
        ignores.phrases.truncate(MAX_RULES);
        for phrase in &mut ignores.phrases {
            phrase.pattern = bounded(&phrase.pattern, MAX_PATTERN);
            phrase.replacement = bounded(&phrase.replacement, MAX_PATTERN);
        }
        ignores.users = login_list(std::mem::take(&mut ignores.users));

        self.filters.truncate(MAX_RULES);
        for filter in &mut self.filters {
            filter.name = bounded(filter.name.trim(), MAX_FILTER_NAME);
            filter.expression = bounded(&filter.expression, MAX_PATTERN);
        }

        self.commands.truncate(MAX_RULES);
        for command in &mut self.commands {
            // One word: the trigger is matched against the first word typed,
            // so a space in it could never match.
            command.trigger = bounded(
                command.trigger.split_whitespace().next().unwrap_or(""),
                MAX_TRIGGER,
            );
            command.template = bounded(&command.template, MAX_PATTERN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebar_defaults_to_an_expanded_left_drawer() {
        let s = SidebarSettings::default();
        assert_eq!(s.position, SidebarPosition::Left);
        assert!(!s.collapsed);
        assert!(
            s.show_offline,
            "hiding follows by default would look like data loss"
        );
        assert!(!s.is_docked(), "a full-width list belongs behind a toggle");
    }

    /// Avatars-only is a permanent switcher, not a drawer you keep reopening.
    #[test]
    fn collapsing_the_sidebar_docks_it() {
        let s = SidebarSettings {
            collapsed: true,
            ..Default::default()
        };
        assert!(s.is_docked());
    }

    /// OBS keeps the URL, so the default must not change under it, and the
    /// overlay must stay off until asked for: it opens a port.
    #[test]
    fn the_overlay_is_off_on_a_fixed_port_by_default() {
        let overlay = OverlaySettings::default();
        assert!(!overlay.enabled);
        assert_eq!(overlay.port, DEFAULT_OVERLAY_PORT);
        assert_eq!(
            overlay.source,
            OverlaySource::Own,
            "following the open channel would move the overlay on a raid click"
        );
        assert!(overlay.hide_commands);
        assert!(overlay.hidden_users.contains(&"nightbot".to_owned()));
    }

    #[test]
    fn the_overlay_is_bounded_and_sanitised() {
        let mut s = Settings::default();
        s.overlay.port = 80;
        s.overlay.font_size = 200;
        s.overlay.bubble_opacity = 250;
        s.overlay.corner_radius = 99;
        s.overlay.max_messages = 0;
        s.overlay.fade_after_seconds = 9999;
        s.overlay.font = ChatFont::Custom;
        s.overlay.custom_font = "Inter\"; } * { display: none".into();
        s.overlay.hidden_users = vec!["@NightBot".into(), "nightbot".into(), " ".into()];
        let o = s.sanitized().overlay;
        assert_eq!(o.port, 1024, "a privileged port cannot be bound anyway");
        assert_eq!(o.font_size, MAX_OVERLAY_FONT_SIZE);
        assert_eq!(o.bubble_opacity, 100);
        assert_eq!(o.corner_radius, MAX_OVERLAY_RADIUS);
        assert_eq!(o.max_messages, 1);
        assert_eq!(o.fade_after_seconds, MAX_OVERLAY_FADE);
        assert_eq!(o.custom_font, "Inter    display none");
        assert_eq!(o.hidden_users, vec!["nightbot".to_owned()]);
    }

    /// A settings file from before the overlay existed keeps everything else.
    #[test]
    fn an_older_file_gains_the_overlay_defaults() {
        let s: Settings = serde_json::from_str(r#"{"chat":{"fontSize":16}}"#).unwrap();
        assert_eq!(s.chat.font_size, 16);
        assert_eq!(s.overlay, OverlaySettings::default());
    }

    #[test]
    fn the_title_bar_is_shown_by_default() {
        assert!(!WindowSettings::default().hide_title_bar);
        assert!(WindowSettings::default().show_header);
    }

    /// The header font is interpolated into CSS the same way the chat's is, so
    /// it gets the same treatment.
    #[test]
    fn the_header_font_is_bounded_and_sanitised() {
        let mut s = Settings::default();
        s.window.header_font_size = 90;
        s.window.header_font = ChatFont::Custom;
        s.window.header_custom_font = "Arial; } body { display: none".into();
        let s = s.sanitized();
        assert_eq!(s.window.header_font_size, MAX_HEADER_FONT_SIZE);
        assert_eq!(s.window.header_custom_font, "Arial  body  display none");

        let mut s = Settings::default();
        s.window.header_font = ChatFont::Custom;
        s.window.header_custom_font = ";;;".into();
        assert_eq!(s.sanitized().window.header_font, ChatFont::System);
    }

    /// A file from before the header was configurable keeps its window choices.
    #[test]
    fn an_older_window_group_gains_the_header_defaults() {
        let older = r#"{"window":{"hideTitleBar":true}}"#;
        let s: Settings = serde_json::from_str(older).unwrap();
        assert!(s.window.hide_title_bar);
        assert!(s.window.show_header);
        assert_eq!(s.window.header_font_size, 15);
    }

    /// Hiding the scroll affordance by default would cost discoverability, so it
    /// is opt-in rather than the default.
    #[test]
    fn scrollbars_are_shown_by_default() {
        assert!(WindowSettings::default().show_scrollbars);
    }

    #[test]
    fn only_the_side_positions_are_vertical() {
        assert!(SidebarPosition::Left.is_vertical());
        assert!(SidebarPosition::Right.is_vertical());
        assert!(!SidebarPosition::Top.is_vertical());
    }

    #[test]
    fn defaults_need_no_configuration() {
        let s = Settings::default();
        assert_eq!(s.theme, ThemePreference::System);
        assert_eq!(s.browser, BrowserSettings::default());
        assert_eq!(s.browser.choice, BrowserChoice::System);
        assert_eq!(s.browser.mode, LaunchMode::Normal);
        assert!(
            !s.browser.new_window,
            "reusing the window is what a browser does by default"
        );
        assert_eq!(s.browser.page, StreamPage::Channel);
        assert!(
            !s.browser.sync_window,
            "an experiment must not be on for someone who never asked for it"
        );
        assert!(
            !s.browser.follow_chat,
            "selecting a channel must not reach the browser unless asked"
        );
        assert_eq!(s.sidebar, SidebarSettings::default());
        assert!(s.chat.show_timestamps);
        assert!(s.chat.show_badges);
        assert_eq!(s.chat.font_size, 13);
        assert!(s.updates.check);
    }

    fn with_font_size(font_size: u8) -> Settings {
        Settings {
            chat: ChatSettings {
                font_size,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn font_size_is_clamped_into_range() {
        assert_eq!(
            with_font_size(200).sanitized().chat.font_size,
            MAX_FONT_SIZE
        );
        assert_eq!(with_font_size(0).sanitized().chat.font_size, MIN_FONT_SIZE);
    }

    #[test]
    fn a_valid_font_size_is_left_alone() {
        assert_eq!(with_font_size(16).sanitized().chat.font_size, 16);
    }

    /// A settings file written by an older build must still load.
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());

        let partial: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(partial.theme, ThemePreference::Dark);
        assert_eq!(partial.chat, ChatSettings::default());
        // A settings file written before these existed must still load.
        assert_eq!(partial.sidebar, SidebarSettings::default());
        assert_eq!(partial.window, WindowSettings::default());
    }

    /// A settings file from a build that predates a field must keep the rest of
    /// the user's choices, not reset everything to defaults.
    #[test]
    fn a_chat_group_missing_newer_fields_keeps_the_older_ones() {
        let older = r#"{"chat":{"fontSize":16,"showTimestamps":false,
                        "showBadges":true,"emoteSize":"large"}}"#;
        let s: Settings = serde_json::from_str(older).unwrap();

        assert_eq!(s.chat.font_size, 16);
        assert!(!s.chat.show_timestamps);
        assert_eq!(s.chat.emote_size, EmoteSize::Large);
        // The fields it could not know about fall back.
        assert!(s.chat.show_status_bar);
        assert_eq!(s.chat.status_bar_position, StatusBarPosition::Bottom);
    }

    #[test]
    fn the_status_bar_sits_above_the_message_box_by_default() {
        let chat = ChatSettings::default();
        assert!(chat.show_status_bar);
        assert_eq!(chat.status_bar_position, StatusBarPosition::Bottom);
    }

    #[test]
    fn chat_defaults_to_the_system_font_with_no_striping() {
        let chat = ChatSettings::default();
        assert_eq!(chat.font, ChatFont::System);
        assert!(chat.custom_font.is_empty());
        assert!(!chat.alternate_backgrounds);
        // On by default: a reader who never opens Settings still wants to know
        // where they left off.
        assert!(chat.show_unread_divider);
        assert!(chat.show_command_button);
        assert!(chat.show_send_button);
        assert!(chat.show_channel_events);
        assert!(chat.show_reply_preview);
        assert!(chat.show_composer_hints);
        assert!(chat.highlight_mentions);
        assert!(chat.highlight_keywords.is_empty());
        // Third-party, so never on unless asked for.
        assert!(!chat.history_on_join);
    }

    /// The name is interpolated into a CSS declaration, so anything that could
    /// end it must not survive.
    #[test]
    fn a_font_name_cannot_escape_its_css_declaration() {
        assert_eq!(
            sanitize_font_name("JetBrains Mono").as_deref(),
            Some("JetBrains Mono")
        );
        assert_eq!(
            sanitize_font_name("Noto-Sans_CJK").as_deref(),
            Some("Noto-Sans_CJK")
        );

        for hostile in [
            "Arial; background: url(evil)",
            "Arial\"; } body { display: none } .x {",
            "Arial'); @import 'evil.css",
            "<script>alert(1)</script>",
        ] {
            let cleaned = sanitize_font_name(hostile).unwrap_or_default();
            for bad in [
                '"', '\'', ';', '{', '}', '(', ')', '<', '>', ':', '/', '@', '\\',
            ] {
                assert!(
                    !cleaned.contains(bad),
                    "{hostile:?} kept {bad:?} -> {cleaned:?}"
                );
            }
        }
    }

    #[test]
    fn a_name_with_nothing_usable_is_refused() {
        assert_eq!(sanitize_font_name(""), None);
        assert_eq!(sanitize_font_name("   "), None);
        assert_eq!(sanitize_font_name(";;;{}"), None);
    }

    #[test]
    fn a_font_name_is_bounded() {
        let long = "A".repeat(500);
        assert_eq!(sanitize_font_name(&long).unwrap().len(), MAX_FONT_NAME);
    }

    /// Choosing "custom" without a usable name would render no text at all.
    #[test]
    fn custom_without_a_usable_name_falls_back_to_the_system_font() {
        let s = Settings {
            chat: ChatSettings {
                font: ChatFont::Custom,
                custom_font: ";;;".into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .sanitized();

        assert_eq!(s.chat.font, ChatFont::System);
        assert!(s.chat.custom_font.is_empty());
    }

    #[test]
    fn a_valid_custom_font_is_kept() {
        let s = Settings {
            chat: ChatSettings {
                font: ChatFont::Custom,
                custom_font: "  JetBrains Mono  ".into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .sanitized();

        assert_eq!(s.chat.font, ChatFont::Custom);
        assert_eq!(s.chat.custom_font, "JetBrains Mono");
    }

    /// The keyword line from before highlight rules existed must survive the
    /// upgrade with the behaviour it had.
    #[test]
    fn old_highlight_words_become_phrases() {
        let older = r#"{"chat":{"highlightKeywords":["poe"," patch notes ",""]},
                        "notifications":{"highlights":true}}"#;
        let s = serde_json::from_str::<Settings>(older).unwrap().sanitized();

        assert!(s.chat.highlight_keywords.is_empty(), "moved, not copied");
        let patterns: Vec<&str> = s
            .highlights
            .phrases
            .iter()
            .map(|p| p.pattern.as_str())
            .collect();
        assert_eq!(patterns, ["poe", "patch notes"]);
        assert!(s.highlights.phrases.iter().all(|p| p.notify && !p.regex));

        // A second pass must not duplicate them.
        assert_eq!(s.clone().sanitized().highlights.phrases.len(), 2);
    }

    #[test]
    fn rule_logins_are_normalised_and_deduplicated() {
        let mut s = Settings::default();
        s.ignores.users = vec![
            " @Forsen ".into(),
            "forsen".into(),
            "".into(),
            "x y!".into(),
        ];
        let s = s.sanitized();
        assert_eq!(s.ignores.users, ["forsen", "xy"]);
    }

    /// The settings screen adds a blank row for the reader to fill in; the save
    /// that follows must not throw it away.
    #[test]
    fn a_blank_rule_row_survives_a_save() {
        let mut s = Settings::default();
        s.highlights.phrases.push(HighlightPhrase::default());
        s.highlights.users.push(HighlightUser::default());
        s.commands.push(CustomCommand::default());
        s.filters.push(ChatFilter::default());
        let s = s.sanitized();
        assert_eq!(s.highlights.phrases.len(), 1);
        assert_eq!(s.highlights.users.len(), 1);
        assert_eq!(s.commands.len(), 1);
        assert_eq!(s.filters.len(), 1);
    }

    #[test]
    fn a_trigger_is_one_word() {
        let mut s = Settings::default();
        s.commands.push(CustomCommand {
            trigger: "  /hi there ".into(),
            template: "Hello {1}".into(),
            in_menu: false,
        });
        assert_eq!(s.sanitized().commands[0].trigger, "/hi");
    }

    #[test]
    fn rule_lists_are_bounded() {
        let mut s = Settings::default();
        s.highlights.phrases = vec![HighlightPhrase::default(); MAX_RULES + 50];
        s.ignores.phrases = vec![
            IgnorePhrase {
                pattern: "x".repeat(MAX_PATTERN * 2),
                ..Default::default()
            };
            1
        ];
        let s = s.sanitized();
        assert_eq!(s.highlights.phrases.len(), MAX_RULES);
        assert_eq!(s.ignores.phrases[0].pattern.chars().count(), MAX_PATTERN);
    }

    #[test]
    fn emote_sizes_are_ordered() {
        assert!(EmoteSize::Small.scale() < EmoteSize::Medium.scale());
        assert!(EmoteSize::Medium.scale() < EmoteSize::Large.scale());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let s = Settings {
            theme: ThemePreference::Light,
            chat: ChatSettings {
                emote_size: EmoteSize::Large,
                show_timestamps: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    /// The browser setting used to be the string `"systemDefault"`. A file
    /// still holding it must load, keeping everything else the user chose.
    #[test]
    fn a_legacy_browser_string_does_not_discard_the_rest() {
        let stored = r#"{"theme":"dark","browser":"systemDefault","chat":{"fontSize":17}}"#;
        let settings: Settings = serde_json::from_str(stored).unwrap();

        assert_eq!(settings.browser, BrowserSettings::default());
        assert_eq!(settings.theme, ThemePreference::Dark);
        assert_eq!(settings.chat.font_size, 17, "the rest must survive");
    }

    #[test]
    fn a_browser_group_round_trips() {
        let settings = Settings {
            browser: BrowserSettings {
                choice: BrowserChoice::Installed {
                    id: "firefox.desktop".into(),
                },
                mode: LaunchMode::Kiosk,
                new_window: true,
                page: StreamPage::Player,
                sync_window: true,
                follow_chat: true,
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }

    #[test]
    fn an_unusable_browser_choice_falls_back_to_the_system() {
        for choice in [
            BrowserChoice::Installed { id: "  ".into() },
            BrowserChoice::Command {
                line: String::new(),
            },
        ] {
            let settings = Settings {
                browser: BrowserSettings {
                    choice,
                    mode: LaunchMode::App,
                    ..Default::default()
                },
                ..Default::default()
            }
            .sanitized();
            assert_eq!(settings.browser.choice, BrowserChoice::System);
            assert_eq!(settings.browser.mode, LaunchMode::App, "mode is kept");
        }
    }

    #[test]
    fn a_command_is_trimmed_and_bounded() {
        let settings = Settings {
            browser: BrowserSettings {
                choice: BrowserChoice::Command {
                    line: format!("  mpv {} %u  ", "x".repeat(MAX_COMMAND_LINE)),
                },
                ..Default::default()
            },
            ..Default::default()
        }
        .sanitized();

        let BrowserChoice::Command { line } = settings.browser.choice else {
            panic!("the command must survive sanitising");
        };
        assert!(line.starts_with("mpv "));
        assert_eq!(line.chars().count(), MAX_COMMAND_LINE);
    }
}
