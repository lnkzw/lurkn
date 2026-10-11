/**
 * The conversation around one chatter, assembled from the log in memory.
 *
 * A user card that lists only what someone said reads as a list of fragments:
 * half of chat is an answer to something. This pulls in the messages their
 * replies answered, and the replies they got, so the card shows an exchange
 * rather than one side of it.
 */

interface Threadable {
  id: string;
  author: { id: string };
  replyTo: string | null;
}

/**
 * Messages involving `userId`, oldest first, in log order.
 *
 * Three things qualify: what they said, what a message of theirs replied to,
 * and what replied to a message of theirs. Order comes from the log rather than
 * from timestamps — chat arrives in order, and a batch shares a timestamp to
 * the second, so sorting by time would shuffle a fast exchange.
 */
export function conversation<T extends Threadable>(messages: T[], userId: string, limit = 12): T[] {
  const mine = new Set<string>();
  for (const m of messages) {
    if (m.author.id === userId) mine.add(m.id);
  }

  const wanted = new Set<string>(mine);

  for (const m of messages) {
    // What they were answering.
    if (mine.has(m.id) && m.replyTo) wanted.add(m.replyTo);
    // What answered them.
    if (m.replyTo && mine.has(m.replyTo)) wanted.add(m.id);
  }

  const picked = messages.filter((m) => wanted.has(m.id));

  // The tail, so the card opens on the most recent exchange. A conversation cut
  // at the front can start mid-reply, which is better than a card that scrolls
  // past the window.
  return picked.slice(-limit);
}
