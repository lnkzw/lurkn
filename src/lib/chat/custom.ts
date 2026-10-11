/**
 * The reader's own commands, expanded in the composer.
 *
 * The trigger is the first word typed, and the template fills in
 *
 *     {1} {2} …      the words after the trigger
 *     {1+} {2+} …    that word and every one after it
 *     {channel.name} {channel.id} {stream.title} {stream.game}
 *     {my.name} {my.id}
 *     {user.name} {user.id} {msg.id} {msg.text}   — from a right-clicked message
 *     {{             a literal {
 *
 * A template that names no word at all gets the words appended, so `/hi` with
 * `Hello!` still passes on whatever followed it. An unknown `{name}` is left as
 * written rather than silently dropped.
 */

export interface Command {
  trigger: string;
  template: string;
}

/** What the variables are, for the settings screen's reference. */
export const COMMAND_VARIABLES: Record<string, string> = {
  '{1} {2} …': 'the first, second … word after the trigger',
  '{1+}': 'the first word and everything after it',
  '{channel.name}': 'the channel being read',
  '{channel.id}': 'its numeric id',
  '{stream.title}': 'its stream title',
  '{stream.game}': 'its category',
  '{my.name}': 'your login',
  '{my.id}': 'your numeric id',
  '{user.name}': 'the author, from a message’s menu',
  '{user.id}': 'their numeric id',
  '{msg.text}': 'the message, from its menu',
  '{msg.id}': 'its id',
  '{{': 'a literal {',
};

/** The variables a command can be filled in from. Absent ones become empty. */
export type CommandVars = Partial<
  Record<
    | 'channel.name'
    | 'channel.id'
    | 'stream.title'
    | 'stream.game'
    | 'my.name'
    | 'my.id'
    | 'user.name'
    | 'user.id'
    | 'msg.id'
    | 'msg.text',
    string
  >
>;

const KNOWN = new Set<string>([
  'channel.name',
  'channel.id',
  'stream.title',
  'stream.game',
  'my.name',
  'my.id',
  'user.name',
  'user.id',
  'msg.id',
  'msg.text',
]);

/**
 * The command a draft starts with, and the words after it.
 *
 * Triggers are compared without case, like Twitch's own commands: `/HI` is
 * what Caps Lock does to `/hi`, not a different command.
 */
export function findCommand<T extends Command>(
  draft: string,
  commands: T[],
): { command: T; args: string[] } | null {
  const words = draft.trim().split(/\s+/);
  const first = words[0]?.toLowerCase() ?? '';
  if (first.length === 0) return null;

  const command = commands.find(
    (c) => c.trigger.trim().length > 0 && c.trigger.trim().toLowerCase() === first,
  );
  return command ? { command, args: words.slice(1) } : null;
}

/** Fill in a template. See the module comment for what it understands. */
export function expand(template: string, args: string[], vars: CommandVars): string {
  let named = false;

  const out = template.replace(
    /\{\{|\{(\d+)(\+?)\}|\{([a-z]+\.[a-z_]+)\}/gi,
    (whole, index: string | undefined, plus: string | undefined, name: string | undefined) => {
      if (whole === '{{') return '{';

      if (index !== undefined) {
        named = true;
        const at = Number(index) - 1;
        if (at < 0) return '';
        return plus ? args.slice(at).join(' ') : (args[at] ?? '');
      }

      const key = (name ?? '').toLowerCase();
      if (!KNOWN.has(key)) return whole;
      return vars[key as keyof CommandVars] ?? '';
    },
  );

  // A template that takes no words passes them on, so `/hi` can wrap a line.
  if (!named && args.length > 0) return `${out} ${args.join(' ')}`.trim();
  return out.trim();
}
