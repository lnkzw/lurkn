# Emote precedence

Normative. `crates/lurkn-emotes/src/precedence.rs` implements this table and its
tests assert it; change both together or neither.

## Resolution order

Twitch-native emotes are **never name-matched**. EventSub delivers the message
pre-split into fragments, and an `emote` fragment is authoritative:

```text
1. Map EventSub fragments directly:
     fragment.emote     -> Emote (provider = Twitch)
     fragment.mention   -> Mention
     fragment.cheermote -> Text (cheer images are not resolved)
     fragment.text      -> continue to step 2
2. Tokenize each text fragment on whitespace.
3. Look each word up in the channel's name -> Emote map.
4. Anything unmatched stays Text.
```

## The name map

Built once per channel. Earlier entries win; later inserts for a name already
present are discarded.

| Rank | Provider | Scope   |
|-----:|----------|---------|
| 0    | 7TV      | channel |
| 1    | BTTV     | channel |
| 2    | FFZ      | channel |
| 3    | 7TV      | global  |
| 4    | BTTV     | global  |
| 5    | FFZ      | global  |

Twitch does not appear in this table — see above.

## Why this order

- **Channel beats global** because it is what the broadcaster deliberately added.
  A channel's own `Pog` should win over any global one.
- **7TV > BTTV > FFZ** matches the de-facto ordering users already have from other
  clients, so a familiar emote does not silently change appearance.

Collisions are common (`OMEGALUL`, `Pog`, `monkaS` exist on several providers), so
the order is asserted in tests rather than left to map-insertion order.

## Failure behaviour

A provider that errors or times out is logged and skipped. Its rank is simply
absent from the map for that channel; everything below it moves up. Twitch
emotes keep rendering when 7TV is down.
