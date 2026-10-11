//! The page OBS loads as a browser source.
//!
//! It builds every message with `createElement` and `textContent`; nothing a
//! chatter typed is ever parsed as markup. The look arrives as data and is set
//! as custom properties, so a settings change restyles the page in place —
//! OBS never has to refresh the source.
//!
//! The class names are the page's public surface: OBS has a Custom CSS field
//! on every browser source, and anything written there targets these.
//! `#chat`, `.msg`, `.head`, `.badge`, `.name`, `.body`, `.emote`, `.mention`
//! and `.action` stay stable.

/// The image hosts Rust already allowlists for emotes and badges.
pub(crate) const IMAGE_HOSTS: &str = "https://static-cdn.jtvnw.net https://cdn.7tv.app https://cdn.betterttv.net https://cdn.frankerfacez.com";

/// Build the page with the current look baked in, so the first paint is
/// already right rather than flashing the defaults.
///
/// `look_json` is serialised by serde; `<` is escaped so no value can close the
/// script element it sits in.
pub fn page_html(nonce: &str, look_json: &str) -> String {
    TEMPLATE
        .replace("__IMAGE_HOSTS__", IMAGE_HOSTS)
        .replace("__NONCE__", nonce)
        .replace("__LOOK__", &look_json.replace('<', "\\u003c"))
}

// Styles are allowed inline (no nonce) on purpose: OBS applies a source's
// Custom CSS by inserting a <style> element, and a nonce-only policy would
// silently drop it. Nothing on this page writes markup, so there is no
// injection point for inline styles to widen.
const TEMPLATE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Lurkn chat overlay</title>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-__NONCE__'; style-src 'unsafe-inline'; img-src __IMAGE_HOSTS__; connect-src 'self'">
<style>
  :root {
    --font: system-ui, sans-serif; --size: 20px; --text: #efeff1;
    --bubble: rgba(24, 24, 27, 0.85); --radius: 12px; --emote: 1.75em;
  }
  html, body { margin: 0; height: 100%; overflow: hidden; background: transparent; }
  #chat {
    position: fixed; inset: 0; box-sizing: border-box; padding: 0.5em;
    display: flex; flex-direction: column; justify-content: flex-end; gap: 0.35em;
    overflow: hidden; font-family: var(--font); font-size: var(--size);
    line-height: 1.35; color: var(--text);
  }
  .msg { flex: none; overflow-wrap: anywhere; transition: opacity 0.4s ease; }
  .msg.out { opacity: 0; }
  .animate .msg { animation: enter 0.25s ease-out; }
  @keyframes enter { from { opacity: 0; transform: translateY(0.6em); } }
  .head { font-weight: 700; }
  .badge { height: 1em; width: auto; vertical-align: -0.12em; margin-right: 0.25em; }
  .no-badges .badge { display: none; }
  .emote-wrap { position: relative; display: inline-block; vertical-align: middle; line-height: 0; }
  .emote { height: var(--emote); width: auto; }
  .emote.zw { position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); }
  .mention { font-weight: 700; }
  .action .body { font-style: italic; }
  .outline #chat {
    text-shadow: 0 0 2px rgba(0, 0, 0, 0.9), 0 0 1px rgba(0, 0, 0, 0.9), 1px 1px 2px rgba(0, 0, 0, 0.6);
  }
  [data-layout="bubbles"] .msg {
    align-self: flex-start; max-width: 100%; box-sizing: border-box;
    padding: 0.4em 0.7em; background: var(--bubble); border-radius: var(--radius);
  }
  [data-layout="bubbles"] .head { display: block; font-size: 0.8em; margin-bottom: 0.1em; }
  [data-layout="plain"] .head { margin-right: 0.3em; }
  [data-layout="plain"] .msg:not(.action) .name::after { content: ":"; }
  /* The browser preview only: a checkerboard says "transparent" where a
     solid colour would be mistaken for part of the design. */
  .demo body {
    background: repeating-conic-gradient(#2a2a2e 0% 25%, #1f1f23 0% 50%) 50% / 24px 24px;
  }
</style>
</head>
<body>
<div id="chat" aria-live="polite"></div>
<script nonce="__NONCE__">
(function () {
  "use strict";
  var root = document.documentElement;
  var list = document.getElementById("chat");
  var look = __LOOK__;

  function hexRgb(hex) {
    var n = parseInt(hex.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  function luminance(rgb) {
    var c = rgb.map(function (v) {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
  }

  function contrast(a, b) {
    var x = luminance(a), y = luminance(b);
    return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
  }

  // What a name is read against: the bubble when there is a solid one, the
  // dark outline when there is not, and otherwise an unknown scene.
  function backdrop() {
    if (look.layout === "bubbles" && look.bubbleOpacity >= 0.5) return hexRgb(look.bubbleColor);
    return look.outline ? [0, 0, 0] : null;
  }

  // Twitch lets anyone pick #0000ff, which vanishes on a dark bubble. Move the
  // colour toward white or black only as far as it takes to read.
  function nameColor(color) {
    if (look.nameColor) return look.nameColor;
    if (!color) return look.textColor;
    var back = backdrop();
    if (!back) return color;
    var target = contrast(back, [255, 255, 255]) >= contrast(back, [0, 0, 0])
      ? [255, 255, 255] : [0, 0, 0];
    var rgb = hexRgb(color);
    for (var i = 0; i < 12 && contrast(rgb, back) < 4.5; i++) {
      rgb = rgb.map(function (v, k) { return Math.round(v + (target[k] - v) * 0.2); });
    }
    return "rgb(" + rgb.join(", ") + ")";
  }

  function paint(item) {
    var name = item.querySelector(".name");
    var color = nameColor(name.getAttribute("data-color"));
    name.style.color = color;
    item.querySelector(".body").style.color = item.classList.contains("action") ? color : "";
  }

  function apply(next) {
    look = next;
    root.setAttribute("data-layout", look.layout);
    root.classList.toggle("outline", look.outline);
    root.classList.toggle("animate", look.animate);
    root.classList.toggle("no-badges", !look.badges);
    var style = root.style;
    style.setProperty("--font", look.font);
    style.setProperty("--size", look.fontSize + "px");
    style.setProperty("--text", look.textColor);
    style.setProperty("--bubble", "rgba(" + hexRgb(look.bubbleColor).join(", ") + ", " + look.bubbleOpacity + ")");
    style.setProperty("--radius", look.radius + "px");
    style.setProperty("--emote", look.emoteScale + "em");
    var items = list.querySelectorAll(".msg");
    for (var i = 0; i < items.length; i++) paint(items[i]);
    trim();
    sweep();
  }

  function image(className, url, alt) {
    var el = document.createElement("img");
    el.className = className;
    el.src = url;
    el.alt = alt;
    el.decoding = "async";
    return el;
  }

  function render(m) {
    var item = document.createElement("div");
    item.className = m.action ? "msg action" : "msg";
    item.setAttribute("data-id", m.id);
    item.setAttribute("data-login", m.login);
    item.setAttribute("data-at", String(m.at));

    var head = document.createElement("span");
    head.className = "head";
    m.badges.forEach(function (badge) { head.appendChild(image("badge", badge.url, badge.title)); });
    var name = document.createElement("span");
    name.className = "name";
    name.textContent = m.name;
    if (m.color) name.setAttribute("data-color", m.color);
    head.appendChild(name);
    item.appendChild(head);

    var body = document.createElement("span");
    body.className = "body";
    var last = null;
    m.parts.forEach(function (part) {
      if (part.kind === "emote") {
        var img = image("emote", part.url, part.name);
        if (part.aspect) img.style.aspectRatio = part.aspect[0] + " / " + part.aspect[1];
        // A zero-width emote stacks onto the one before it.
        if (part.zeroWidth && last) {
          img.classList.add("zw");
          last.appendChild(img);
          return;
        }
        var wrap = document.createElement("span");
        wrap.className = "emote-wrap";
        wrap.appendChild(img);
        body.appendChild(wrap);
        last = wrap;
        return;
      }
      if (part.kind === "mention") {
        var mention = document.createElement("span");
        mention.className = "mention";
        mention.textContent = part.text;
        body.appendChild(mention);
      } else {
        body.appendChild(document.createTextNode(part.text));
      }
      // The space between two emotes is a text part of its own; a zero-width
      // emote after it still belongs to the emote before.
      if (/\S/.test(part.text)) last = null;
    });
    item.appendChild(body);
    paint(item);
    return item;
  }

  function add(m) {
    // A replayed message that had already faded stays gone.
    if (look.fadeAfter > 0 && Date.now() - m.at >= look.fadeAfter * 1000) return;
    list.appendChild(render(m));
    trim();
  }

  function live() {
    return list.querySelectorAll(".msg:not(.out)");
  }

  function trim() {
    var items = live();
    for (var i = 0; i < items.length - look.maxMessages; i++) items[i].remove();
  }

  function fade(item) {
    item.classList.add("out");
    setTimeout(function () { item.remove(); }, 400);
  }

  function sweep() {
    if (!(look.fadeAfter > 0)) return;
    var cutoff = Date.now() - look.fadeAfter * 1000;
    var items = live();
    for (var i = 0; i < items.length; i++) {
      if (Number(items[i].getAttribute("data-at")) <= cutoff) fade(items[i]);
    }
  }

  function removeWhere(attribute, value) {
    var items = list.querySelectorAll(".msg");
    for (var i = 0; i < items.length; i++) {
      if (items[i].getAttribute(attribute) === value) items[i].remove();
    }
  }

  apply(look);
  setInterval(sweep, 500);

  // EventSource retries on its own, so a source left in a scene while Lurkn
  // restarts picks up again once it is back on the same port.
  var events = new EventSource("/events");
  events.addEventListener("open", function () {
    // The server replays recent messages to every new connection; start
    // clean so a reconnect does not draw them twice.
    var items = list.querySelectorAll(".msg:not(.demo-msg)");
    for (var i = 0; i < items.length; i++) items[i].remove();
  });
  events.addEventListener("look", function (e) { apply(JSON.parse(e.data)); });
  events.addEventListener("message", function (e) { add(JSON.parse(e.data)); });
  events.addEventListener("delete", function (e) { removeWhere("data-id", JSON.parse(e.data)); });
  events.addEventListener("user", function (e) { removeWhere("data-login", JSON.parse(e.data)); });
  events.addEventListener("clear", function () { list.textContent = ""; });

  // `?demo` is the preview Lurkn opens in a browser: sample lines on a loop,
  // so a setting can be judged without waiting for chat to say something.
  if (new URLSearchParams(location.search).has("demo")) {
    root.classList.add("demo");
    var KAPPA = "https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/3.0";
    var samples = [
      ["Lurkn", "#9146ff", false, [["text", "This is the overlay preview. Change a setting in Lurkn and watch it here."]]],
      ["chatter_one", "#1e90ff", false, [["text", "hello chat "], ["emote", "Kappa"]]],
      ["DarkBlueName", "#0000ff", false, [["text", "I picked dark blue; the overlay lifts it until it reads."]]],
      ["mod_person", "#00ff7f", false, [["mention", "@chatter_one"], ["text", " a longer line, to show how a message wraps once it runs past the width of the source in OBS"]]],
      ["someone", "#ff69b4", true, [["text", "waves at the stream"]]]
    ];
    var n = 0;
    var tick = function () {
      var s = samples[n % samples.length];
      n += 1;
      var item = render({
        id: "demo-" + n, login: s[0].toLowerCase(), name: s[0], color: s[1], action: s[2],
        badges: [], at: Date.now(),
        parts: s[3].map(function (p) {
          return p[0] === "emote"
            ? { kind: "emote", name: p[1], url: KAPPA, aspect: [28, 28], zeroWidth: false }
            : { kind: p[0], text: p[1] };
        })
      });
      item.classList.add("demo-msg");
      list.appendChild(item);
      trim();
    };
    tick();
    setInterval(tick, 2500);
  }
})();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> String {
        page_html("n0nce", r#"{"layout":"bubbles"}"#)
    }

    #[test]
    fn the_look_is_baked_in() {
        assert!(page().contains(r#"var look = {"layout":"bubbles"};"#));
    }

    /// A value can never close the script element the look sits in.
    #[test]
    fn the_look_cannot_end_its_script() {
        let html = page_html("n0nce", r#"{"font":"</script><script>alert(1)//"}"#);
        assert!(!html.contains("</script><script>alert"));
        assert!(html.contains(r#"</script>"#));
    }

    /// The overlay may draw emotes and badges from the providers and talk back
    /// to Lurkn; it may not load script from anywhere.
    #[test]
    fn the_policy_is_tight() {
        let html = page();
        assert!(html.contains("default-src 'none'"));
        assert!(html.contains("script-src 'nonce-n0nce'"));
        assert!(html.contains("connect-src 'self'"));
        assert!(html.contains("img-src https://static-cdn.jtvnw.net https://cdn.7tv.app"));
        assert_eq!(html.matches(r#"<script nonce="n0nce">"#).count(), 1);
    }

    /// OBS's Custom CSS arrives as an inserted <style>; a nonce on style-src
    /// would block it.
    #[test]
    fn obs_custom_css_is_not_blocked() {
        assert!(page().contains("style-src 'unsafe-inline'"));
    }

    /// Chat text must never be parsed as markup.
    #[test]
    fn nothing_is_written_as_html() {
        let html = page();
        assert!(!html.contains("innerHTML"));
        assert!(!html.contains("insertAdjacentHTML"));
        assert!(!html.contains("document.write"));
        assert!(html.contains("textContent = m.name"));
    }

    #[test]
    fn the_page_is_transparent_for_obs() {
        assert!(page().contains("background: transparent"));
    }
}
