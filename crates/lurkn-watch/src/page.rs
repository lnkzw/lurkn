//! The page the browser holds open.
//!
//! It exists so the browser never has to navigate: the window stays on this
//! page for the whole session and only the embedded player changes channel.
//! Navigating to twitch.tv instead would tear down the connection that makes
//! switching possible in the first place.
//!
//! Beside the player sits Lurkn's chat, read-only, the way Twitch's own page
//! has it: a column down the right edge that folds away. It builds every line
//! with `createElement` and `textContent`; nothing a chatter typed is ever
//! parsed as markup.

use lurkn_core::Login;

use crate::overlay::page::IMAGE_HOSTS;

/// Hosts Twitch will accept as the embedding page. Both spellings, because
/// which one the browser reports depends on how the URL was opened.
const PARENTS: &str = r#"["localhost","127.0.0.1"]"#;

/// Build the watch page.
///
/// `token` and `nonce` are hex from the system's random source, and `channel`
/// is a validated [`Login`], so none of those needs escaping. `look_json` is
/// serialised by serde from a sanitised look, or `null`; `<` is escaped so no
/// value can close the script element it sits in. It is substituted last, so
/// nothing in it can be mistaken for another placeholder.
pub fn page_html(token: &str, nonce: &str, channel: Option<&Login>, look_json: &str) -> String {
    let initial = match channel {
        Some(login) => format!("\"{}\"", login.as_str()),
        None => "null".to_owned(),
    };

    TEMPLATE
        .replace("__IMAGE_HOSTS__", IMAGE_HOSTS)
        .replace("__NONCE__", nonce)
        .replace("__TOKEN__", token)
        .replace("__PARENTS__", PARENTS)
        .replace("__INITIAL__", &initial)
        .replace("__LOOK__", &look_json.replace('<', "\\u003c"))
}

// The chat's rules mirror `ChatMessage.svelte`, in `em` rather than `rem` so
// that one font size scales the whole column — on a TV the time and the
// padding have to grow with the text, not stay desktop-sized.
const TEMPLATE: &str = r##"<!doctype html>
<html lang="en" class="collapsed">
<head>
<meta charset="utf-8">
<title>Lurkn</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-__NONCE__' https://player.twitch.tv; frame-src https://player.twitch.tv; style-src 'nonce-__NONCE__'; img-src __IMAGE_HOSTS__; connect-src 'self'">
<style nonce="__NONCE__">
  :root {
    --bg: #0e0e10; --bg-raised: #18181b; --border: #2a2a2e; --text: #efeff1;
    --text-dim: #adadb8; --accent: #9146ff; --highlight: #9146ff; --ok: #00c08b;
    --warn: #ffb020; --chat-font: system-ui, sans-serif; --size: 13px; --emote: 1.75em;
    color-scheme: dark;
  }
  html, body { height: 100%; margin: 0; overflow: hidden; background: #0e0e10;
    color: #adadb8; font: 14px/1.5 system-ui, sans-serif; }
  body:focus { outline: none; }
  #stage { display: flex; height: 100%; }
  #video { position: relative; flex: 1; min-width: 0; }
  #player, #player iframe { width: 100%; height: 100%; border: 0; display: block; }
  #idle { position: absolute; inset: 0; margin: 0; display: grid; place-items: center; }
  #idle[hidden] { display: none; }
  #lost { position: absolute; top: 0; left: 0; right: 0; z-index: 3; display: flex;
    align-items: center; gap: 12px; padding: 8px 12px; background: #18181b;
    border-bottom: 1px solid #2a2a2e; color: #efeff1; }
  #lost[hidden] { display: none; }
  #lost button { margin-left: auto; font: inherit; padding: 4px 10px;
    border: 1px solid #2a2a2e; border-radius: 6px; background: #0e0e10;
    color: #efeff1; cursor: pointer; }

  /* As wide as Twitch's column at the default size, and wider as the text
     grows, so a bigger font does not just mean more wrapping. */
  #chat { position: relative; flex: none; width: min(26em, 45vw); display: flex;
    flex-direction: column; font-family: var(--chat-font); font-size: var(--size);
    background: var(--bg); color: var(--text); border-left: 1px solid var(--border); }
  .collapsed #chat { display: none; }
  #log { flex: 1; min-height: 0; overflow-x: hidden; overflow-y: auto; padding: 0.3em 0;
    scrollbar-width: thin; scrollbar-color: transparent transparent; }
  #log:hover { scrollbar-color: color-mix(in srgb, var(--text) 25%, transparent) transparent; }
  #log[hidden], #elsewhere[hidden] { display: none; }
  #elsewhere { margin: auto 1em; color: var(--text-dim); text-align: center; }
  #elsewhere strong { color: var(--text); }

  /* The arrow that hides or shows the chat sits on the video, at its right
     edge — beside the chat rather than over its lines — and shows only while
     the mouse moves, so a window left on a TV is the stream and the chat and
     nothing else. One spot, one arrow at a time: hide while the chat is open,
     show while it is folded away.

     Nothing of the page may lie over the video while the mouse is away.
     Twitch's player will not start on its own while anything covers it — an
     invisible button counts — and a strip kept over the video to notice the
     mouse left every switch paused, waiting for a click on play. Over the video
     the page is told nothing, not even that the mouse arrived, so with the
     chat folded away a narrow rail beside the video is the page's own, there
     to notice it; black, to pass for the player's letterbox. */
  #rail { display: none; flex: none; width: 1rem; background: #000; }
  .collapsed #rail { display: block; }
  .edge { position: absolute; top: 0.5rem; right: 0.5rem; z-index: 2; width: 2.25rem;
    height: 2.25rem; padding: 0; display: none; place-items: center;
    border: 1px solid var(--border); border-radius: 6px;
    background: color-mix(in srgb, var(--bg-raised) 85%, transparent);
    color: var(--text); cursor: pointer; }
  html:not(.collapsed).awake #collapse, html:not(.collapsed) #collapse:hover,
  html:not(.collapsed) #collapse:focus-visible, html.collapsed.awake #expand,
  html.collapsed #expand:hover, html.collapsed #expand:focus-visible { display: grid; }
  .edge svg { width: 1.25rem; height: 1.25rem; }

  .msg { padding: 0.18em 0.9em; line-height: 1.45; overflow-wrap: anywhere; }
  .striped .odd { background: color-mix(in srgb, var(--text) 4%, transparent); }
  .no-time .time, .no-badges .badge, .no-replies .replying, .no-events .event { display: none; }
  .time { color: var(--text-dim); font-size: 0.85em; font-variant-numeric: tabular-nums;
    margin-right: 0.5em; }
  .badge { height: 1.1em; width: 1.1em; vertical-align: middle; margin-right: 0.2em; }
  .author { font-weight: 600; }
  .colon { color: var(--text-dim); margin-right: 0.25em; }
  .body.action { font-style: italic; }
  .emote { height: var(--emote); vertical-align: middle; margin: 0 0.05em; }
  .emote.zw { margin-left: calc(var(--emote) * -1.03); position: relative; z-index: 1; }
  .mention { font-weight: 600; border-radius: 3px; padding: 0 0.2em;
    background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .replying { display: block; font-size: 0.85em; color: var(--text-dim); white-space: nowrap;
    overflow: hidden; text-overflow: ellipsis; }
  .replying.gone { font-style: italic; }
  .removed { display: none; }
  .deleted .body, .deleted .author, .deleted .time { opacity: 0.45; }
  .deleted .body { text-decoration: line-through; }
  .deleted .removed { display: inline-block; margin-right: 0.35em; padding: 0 0.3em;
    font-size: 0.75em; color: var(--text-dim); border: 1px solid var(--border);
    border-radius: 3px; text-decoration: none; }
  .highlight { background: color-mix(in srgb, var(--highlight) 14%, transparent);
    box-shadow: inset 3px 0 0 var(--highlight); }
  .msg.marked { border-left: 3px solid var(--mark, var(--highlight)); padding-left: calc(0.9em - 3px);
    background: color-mix(in srgb, var(--mark, var(--highlight)) 12%, transparent); }
  .msg.notice { --event: var(--accent); border-left: 3px solid var(--event);
    padding-left: calc(0.9em - 3px); background: color-mix(in srgb, var(--event) 9%, transparent); }
  .notice[data-event="raid"], .notice[data-event="charity"] { --event: var(--ok); }
  .notice[data-event="bitsBadge"] { --event: var(--warn); }
  .notice[data-event="other"] { --event: var(--text-dim); }
  .notice-text { display: block; font-weight: 600; color: var(--text); }
  .bits { display: inline-block; margin-right: 0.3em; padding: 0 0.35em; border-radius: 999px;
    background: color-mix(in srgb, var(--warn) 22%, transparent); color: var(--warn);
    font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
</head>
<body tabindex="-1">
<div id="stage">
  <div id="video">
    <button type="button" class="edge" id="collapse" title="Hide chat" aria-label="Hide chat">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17 12H3"/><path d="m11 18 6-6-6-6"/><path d="M21 5v14"/></svg>
    </button>
    <button type="button" class="edge" id="expand" title="Show chat" aria-label="Show chat">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m9 6-6 6 6 6"/><path d="M3 12h14"/><path d="M21 19V5"/></svg>
    </button>
    <div id="player"></div>
    <p id="idle">Pick a channel in Lurkn.</p>
  </div>
  <aside id="chat" aria-label="Chat">
    <p id="elsewhere" hidden>Lurkn is reading <strong id="reading"></strong>, not this stream.</p>
    <div id="log"></div>
  </aside>
  <div id="rail"></div>
</div>
<div id="lost" hidden>
  <span>Lurkn is no longer running, so this window has stopped following it. The stream keeps playing.</span>
  <button type="button" id="dismiss">Dismiss</button>
</div>
<script nonce="__NONCE__" src="https://player.twitch.tv/js/embed/v1.js"></script>
<script nonce="__NONCE__">
(function () {
  "use strict";
  var TOKEN = "__TOKEN__";
  var PARENTS = __PARENTS__;
  var root = document.documentElement;
  var mount = document.getElementById("player");
  var idle = document.getElementById("idle");
  var log = document.getElementById("log");
  var elsewhere = document.getElementById("elsewhere");
  var player = null;
  var frame = null;
  // The channel in the player, and the one Lurkn's chat is on. The chat is
  // only shown while they agree: one stream beside another's chat misleads.
  var playing = null;
  var reading = null;

  // The embed script gives us setChannel, which swaps the stream without
  // rebuilding the player. Without it a channel change is a fresh iframe, i.e.
  // a full reload — still correct, just slower and it loses the volume.
  function hasApi() {
    return typeof window.Twitch !== "undefined" && typeof window.Twitch.Player === "function";
  }

  function show(channel) {
    // The server replays the channel to every connection, the first one
    // included, so the player is often asked for the stream it already has.
    if (!channel || channel === playing) return;
    document.title = channel + " — Lurkn";
    idle.hidden = true;
    playing = channel;
    agree();

    if (hasApi()) {
      if (player) {
        player.setChannel(channel);
      } else {
        player = new window.Twitch.Player("player", {
          channel: channel,
          parent: PARENTS,
          width: "100%",
          height: "100%"
        });
      }
      return;
    }

    if (!frame) {
      frame = document.createElement("iframe");
      frame.allow = "autoplay; fullscreen";
      frame.allowFullscreen = true;
      mount.appendChild(frame);
    }
    frame.src = "https://player.twitch.tv/?channel=" + encodeURIComponent(channel) +
      "&parent=localhost&parent=127.0.0.1";
  }

  function agree() {
    var apart = reading !== null && playing !== null && reading !== playing;
    elsewhere.hidden = !apart;
    log.hidden = apart;
    document.getElementById("reading").textContent = reading || "";
    if (!apart) toBottom();
  }

  // ---- The look: Lurkn's, sent as data and set as custom properties, so a
  // settings change restyles the column in place.

  var COLORS = {
    background: "--bg", surface: "--bg-raised", border: "--border", text: "--text",
    textDim: "--text-dim", accent: "--accent", highlight: "--highlight", ok: "--ok", warn: "--warn"
  };

  function apply(look) {
    if (!look) return;
    var style = root.style;
    Object.keys(COLORS).forEach(function (key) { style.setProperty(COLORS[key], look[key]); });
    style.setProperty("--chat-font", look.font);
    style.setProperty("--size", look.fontSize + "px");
    style.setProperty("--emote", look.emoteScale + "em");
    style.setProperty("color-scheme", look.scheme);
    root.classList.toggle("no-time", !look.timestamps);
    root.classList.toggle("no-badges", !look.badges);
    root.classList.toggle("striped", look.alternate);
    root.classList.toggle("no-events", !look.channelEvents);
    root.classList.toggle("no-replies", !look.replies);
    toBottom();
  }

  // ---- The log.

  var MAX = 150;
  // Who said what, by message id, for the line above a reply.
  var said = new Map();
  // Stripes are counted from where a line was added, not from its position,
  // so trimming the top does not make every row change colour.
  var after = 0;
  var before = 0;
  // Stay on the newest line unless the reader scrolled up to look back.
  var pinned = true;

  function toBottom() {
    if (pinned && !log.hidden) log.scrollTop = log.scrollHeight;
  }

  log.addEventListener("scroll", function () {
    pinned = log.scrollHeight - log.scrollTop - log.clientHeight < 24;
  });
  window.addEventListener("resize", toBottom);

  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function image(className, url, alt) {
    var img = el("img", className);
    img.src = url;
    img.alt = alt;
    img.title = alt;
    img.decoding = "async";
    // An image arriving grows its line; keep the newest one in view.
    img.addEventListener("load", toBottom);
    return img;
  }

  function pad(n) { return (n < 10 ? "0" : "") + n; }

  function render(m, odd) {
    var notice = m.event && m.event.text.length > 0 ? m.event : null;
    var item = el("div", "msg");
    item.setAttribute("data-id", m.id);
    item.setAttribute("data-login", m.login);
    if (odd) item.classList.add("odd");
    if (m.deleted) item.classList.add("deleted");
    if (m.kind === "highlight" && !notice) item.classList.add("highlight");
    if (m.marked) {
      item.classList.add("marked");
      if (m.mark) item.style.setProperty("--mark", m.mark);
    }
    if (notice) {
      item.classList.add("notice");
      item.setAttribute("data-event", notice.kind);
    }
    // A cheer is a chatter's own message; only real channel events can be
    // switched off.
    if (m.event && m.event.kind !== "cheer") item.classList.add("event");

    if (m.replyTo) {
      var parent = said.get(m.replyTo);
      item.appendChild(parent
        ? el("div", "replying", "↩ @" + parent.name + ": " + parent.text)
        : el("div", "replying gone", "↩ a message no longer shown"));
    }

    var at = new Date(m.at);
    item.appendChild(el("span", "time", pad(at.getHours()) + ":" + pad(at.getMinutes())));
    m.badges.forEach(function (badge) { item.appendChild(image("badge", badge.url, badge.title)); });

    if (notice) item.appendChild(el("span", "notice-text", notice.text));

    if (!notice || m.parts.length > 0) {
      var name = el("span", "author", m.name);
      name.style.color = m.color || "var(--text)";
      item.appendChild(name);
      item.appendChild(el("span", "colon", ":"));
    }

    if (m.event && m.event.kind === "cheer" && m.event.count) {
      item.appendChild(el("span", "bits", m.event.count.toLocaleString() + " bits"));
    }

    var body = el("span", m.kind === "action" ? "body action" : "body");
    body.appendChild(el("span", "removed", "message removed"));
    m.parts.forEach(function (part) {
      if (part.kind === "emote") {
        var img = image(part.zeroWidth ? "emote zw" : "emote", part.url, part.name);
        if (part.aspect) img.style.aspectRatio = part.aspect[0] + " / " + part.aspect[1];
        body.appendChild(img);
      } else if (part.kind === "mention") {
        body.appendChild(el("span", "mention", part.text));
      } else {
        body.appendChild(document.createTextNode(part.text));
      }
    });
    item.appendChild(body);
    return item;
  }

  function remember(m) {
    said.set(m.id, { name: m.name, text: m.text });
  }

  function trim() {
    while (log.childElementCount > MAX) {
      said.delete(log.firstElementChild.getAttribute("data-id"));
      log.firstElementChild.remove();
    }
  }

  function add(list) {
    list.forEach(function (m) {
      remember(m);
      log.appendChild(render(m, after++ % 2 === 1));
    });
    trim();
    toBottom();
  }

  // Backfill goes above what is already there. Remembered first, so a reply
  // inside it can name a parent that is older still.
  function prepend(list) {
    list.forEach(remember);
    var first = log.firstElementChild;
    for (var i = list.length - 1; i >= 0; i--) {
      before -= 1;
      log.insertBefore(render(list[i], Math.abs(before % 2) === 1), first);
      first = log.firstElementChild;
    }
    trim();
    toBottom();
  }

  function mark(attribute, value) {
    var items = log.querySelectorAll(".msg");
    for (var i = 0; i < items.length; i++) {
      if (items[i].getAttribute(attribute) === value) items[i].classList.add("deleted");
    }
  }

  function reset() {
    log.textContent = "";
    said.clear();
    after = 0;
    before = 0;
    pinned = true;
  }

  // ---- Folding the column away, and the arrows that do it.

  document.getElementById("collapse").addEventListener("click", function () {
    root.classList.add("collapsed");
  });
  document.getElementById("expand").addEventListener("click", function () {
    root.classList.remove("collapsed");
    pinned = true;
    toBottom();
  });

  var sleep = null;
  function wake() {
    root.classList.add("awake");
    clearTimeout(sleep);
    sleep = setTimeout(function () { root.classList.remove("awake"); }, 2000);
  }
  // Out of the layout while asleep, so out of the tab order too; Tab brings
  // the arrow back first, and it stays while it has the focus.
  window.addEventListener("keydown", function (event) {
    if (event.key === "Tab") wake();
  });
  document.addEventListener("mousemove", function () {
    wake();
    // A click on the player hands it the keyboard, and Twitch's frame never
    // passes a key back — so Ctrl and + would zoom the browser instead of the
    // chat. The mouse being over this page again means the player is no
    // longer being used; take the keyboard back.
    var active = document.activeElement;
    if (active && active.tagName === "IFRAME") document.body.focus({ preventScroll: true });
  });

  // ---- Ctrl and + or -: Lurkn's chat font, here and in Lurkn at once. The
  // page only asks; the new size comes back as a look, like any other change.

  window.addEventListener("keydown", function (event) {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    var step = event.key === "+" || event.key === "=" ? "bigger"
      : event.key === "-" || event.key === "_" ? "smaller" : null;
    if (!step) return;
    event.preventDefault();
    fetch("/font?t=" + TOKEN + "&step=" + step, { method: "POST" }).catch(function () {});
  });

  // ---- The link to Lurkn.
  //
  // EventSource reconnects on its own, and the server replays the current
  // channel to every new connection, so a dropped link repairs itself — while
  // Lurkn is running.
  //
  // It cannot repair itself across a Lurkn restart: the port is ephemeral and
  // the token is made fresh each session, so the retries never land. Rather
  // than leave a window that looks fine and silently ignores every channel
  // change, say so once the retries have clearly stopped being a blip.
  var lost = document.getElementById("lost");
  var failures = 0;
  var GIVE_UP = 3;

  document.getElementById("dismiss").addEventListener("click", function () {
    lost.hidden = true;
  });

  var events = new EventSource("/events?t=" + TOKEN);
  events.addEventListener("channel", function (event) {
    // A delivered event means the link is good, whatever happened before it.
    failures = 0;
    lost.hidden = true;
    show(event.data);
  });
  events.addEventListener("open", function () {
    failures = 0;
    lost.hidden = true;
  });
  events.addEventListener("error", function () {
    // One error is an ordinary reconnect; several in a row is Lurkn gone.
    failures += 1;
    if (failures >= GIVE_UP) lost.hidden = false;
  });
  events.addEventListener("look", function (e) { apply(JSON.parse(e.data)); });
  // Sent on every connection and every switch, ahead of that channel's lines,
  // so a reconnect starts clean rather than drawing the replay twice.
  events.addEventListener("chat", function (e) {
    reset();
    reading = JSON.parse(e.data);
    agree();
  });
  events.addEventListener("messages", function (e) { add(JSON.parse(e.data)); });
  events.addEventListener("history", function (e) { prepend(JSON.parse(e.data)); });
  events.addEventListener("delete", function (e) { mark("data-id", JSON.parse(e.data)); });
  events.addEventListener("user", function (e) { mark("data-login", JSON.parse(e.data)); });
  events.addEventListener("clear", reset);

  apply(__LOOK__);
  show(__INITIAL__);
})();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> String {
        page_html(
            "tok3n",
            "n0nce",
            Some(&Login::parse("forsen").unwrap()),
            r#"{"fontSize":13}"#,
        )
    }

    /// The window must say when it has stopped following, rather than looking
    /// fine while ignoring every channel change.
    #[test]
    fn it_warns_once_the_link_is_clearly_gone() {
        let html = page();
        assert!(html.contains(r#"<div id="lost" hidden>"#));
        assert!(html.contains("no longer running"));
        assert!(
            html.contains("failures >= GIVE_UP"),
            "the notice waits for repeated failures, not the first blip"
        );
        assert!(
            html.contains(r#"events.addEventListener("open""#),
            "a recovered link clears the notice"
        );
    }

    #[test]
    fn the_initial_channel_is_baked_in() {
        assert!(page().contains("show(\"forsen\")"));
    }

    #[test]
    fn with_no_channel_it_waits() {
        let html = page_html("tok3n", "n0nce", None, "null");
        assert!(html.contains("show(null)"));
        assert!(html.contains("apply(null)"));
        assert!(html.contains("Pick a channel in Lurkn"));
    }

    #[test]
    fn the_event_stream_carries_the_token() {
        assert!(page().contains(r#"new EventSource("/events?t=" + TOKEN)"#));
        assert!(page().contains(r#"var TOKEN = "tok3n""#));
    }

    /// A font step changes a setting: it is a POST, and it carries the token
    /// the server checks.
    #[test]
    fn a_font_step_posts_with_the_token() {
        assert!(
            page().contains(r#"fetch("/font?t=" + TOKEN + "&step=" + step, { method: "POST" })"#)
        );
    }

    #[test]
    fn the_look_is_baked_in() {
        assert!(page().contains(r#"apply({"fontSize":13});"#));
    }

    /// A value can never close the script element the look sits in.
    #[test]
    fn the_look_cannot_end_its_script() {
        let html = page_html("t", "n", None, r#"{"font":"</script><script>alert(1)//"}"#);
        assert!(!html.contains("</script><script>alert"));
    }

    /// Something in the look that happens to spell a placeholder stays text.
    #[test]
    fn the_look_cannot_pull_in_the_token() {
        let html = page_html("tok3n", "n0nce", None, r#"{"font":"__TOKEN__"}"#);
        assert!(html.contains(r#"apply({"font":"__TOKEN__"});"#));
    }

    /// Without the nonce the inline script the page depends on is blocked by
    /// its own policy, and the page renders as a blank window.
    #[test]
    fn every_inline_block_carries_the_nonce() {
        let html = page();
        assert!(html.contains("script-src 'nonce-n0nce' https://player.twitch.tv"));
        assert_eq!(
            html.matches("nonce=\"n0nce\"").count(),
            3,
            "the style and both scripts need it"
        );
    }

    /// The policy is the page's own guard: it may talk to Twitch's player and
    /// back to Lurkn, draw emotes and badges from their providers, and nothing
    /// else.
    #[test]
    fn the_policy_allows_only_twitch_and_this_server() {
        let html = page();
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("frame-src https://player.twitch.tv"));
        assert!(html.contains("connect-src 'self'"));
        assert!(html.contains("img-src https://static-cdn.jtvnw.net https://cdn.7tv.app"));
    }

    /// Chat text must never be parsed as markup.
    #[test]
    fn nothing_is_written_as_html() {
        let html = page();
        assert!(!html.contains("innerHTML"));
        assert!(!html.contains("insertAdjacentHTML"));
        assert!(!html.contains("document.write"));
    }

    /// Twitch's layout: the chat down the right edge, folded away to begin
    /// with, so the window opens as the stream alone.
    #[test]
    fn the_chat_starts_folded_on_the_right() {
        let html = page();
        assert!(
            html.contains(r#"<html lang="en" class="collapsed">"#),
            "folded at load"
        );
        let video = html.find(r#"<div id="video">"#).unwrap();
        let chat = html.find(r#"<aside id="chat""#).unwrap();
        assert!(video < chat);
    }

    /// Twitch's player does not start on its own while anything of the page
    /// lies over it, an invisible button included. The arrows are out of the
    /// layout until the mouse comes, and what notices the mouse with the chat
    /// folded away sits beside the video rather than on it.
    #[test]
    fn nothing_lies_over_the_player_while_the_mouse_is_away() {
        let html = page();
        let at = html.find(".edge {").unwrap();
        let rule = &html[at..at + html[at..].find('}').unwrap()];
        assert!(rule.contains("display: none"), "{rule}");
        assert!(!html.contains(r#"id="zone""#));

        let video = html.find(r#"<div id="video">"#).unwrap();
        let chat = html.find(r#"<aside id="chat""#).unwrap();
        let rail = html.find(r#"<div id="rail">"#).unwrap();
        assert!(video < chat && chat < rail, "the rail is outside the video");
    }

    /// Both arrows sit on the video, beside the chat: one over the chat's
    /// first lines would cover them whenever the mouse moves.
    #[test]
    fn the_arrows_sit_outside_the_chat() {
        let html = page();
        let chat = html.find(r#"<aside id="chat""#).unwrap();
        for id in [r#"id="collapse""#, r#"id="expand""#] {
            assert!(html.find(id).unwrap() < chat, "{id}");
        }
    }
}
