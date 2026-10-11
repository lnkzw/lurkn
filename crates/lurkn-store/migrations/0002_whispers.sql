-- Whispers, threaded by the other person.
--
-- Twitch serves no whisper history: EventSub delivers them live and there is no
-- endpoint to ask for what was missed. What this table holds is therefore the
-- only record that will ever exist, which is why a whisper is written the
-- moment it arrives rather than kept in memory.
CREATE TABLE IF NOT EXISTS whispers (
    id          TEXT PRIMARY KEY,
    -- The other party, whichever direction the message went.
    peer_id     TEXT NOT NULL,
    peer_login  TEXT NOT NULL,
    peer_name   TEXT NOT NULL,
    -- 1 when this account sent it, 0 when it was received.
    outgoing    INTEGER NOT NULL,
    body        TEXT NOT NULL,
    sent_at     TEXT NOT NULL,
    -- 0 until the thread has been opened.
    read        INTEGER NOT NULL DEFAULT 0
);

-- Threads are listed newest-first and opened one peer at a time.
CREATE INDEX IF NOT EXISTS whispers_peer_time ON whispers (peer_id, sent_at);
CREATE INDEX IF NOT EXISTS whispers_time ON whispers (sent_at);
