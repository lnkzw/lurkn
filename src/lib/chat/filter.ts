/**
 * A small expression language for narrowing what the chat shows:
 *
 *     author.badges contains "moderator" || flags.highlighted
 *     !(message.content match r"^!\w+")
 *     author.subbed && author.sub_length >= 12
 *
 * Parsing and evaluating are separate so a filter is parsed once, when it is
 * saved or switched on, and then run against every message. Both are pure.
 *
 * String comparisons ignore case: nobody filtering for "moderator" means to
 * miss "Moderator". A regex is case-sensitive unless it
 * is written `ri"…"`.
 */

import type { ChatMessage } from '$lib/ipc';

export type Value = boolean | number | string | RegExp | Value[];

type BinaryOp =
  | '||'
  | '&&'
  | '=='
  | '!='
  | '<'
  | '<='
  | '>'
  | '>='
  | 'contains'
  | 'startswith'
  | 'endswith'
  | 'match'
  | '+'
  | '-'
  | '*'
  | '/'
  | '%';

export type Expr =
  | { kind: 'literal'; value: Value }
  | { kind: 'var'; name: string }
  | { kind: 'not'; operand: Expr }
  | { kind: 'neg'; operand: Expr }
  | { kind: 'list'; items: Expr[] }
  | { kind: 'binary'; op: BinaryOp; left: Expr; right: Expr };

export type Parsed = { ok: true; expr: Expr } | { ok: false; error: string; at: number };

/**
 * What a filter can ask about a message, and what each one is. Doubles as the
 * reference the settings screen shows.
 */
export const VARIABLES: Record<string, string> = {
  'author.badges': 'badge ids, e.g. "moderator", "vip", "subscriber"',
  'author.color': 'name colour as "#rrggbb", or ""',
  'author.name': 'display name',
  'author.login': 'login name (Lurkn)',
  'author.no_color': 'never picked a name colour',
  'author.subbed': 'subscriber or founder',
  'author.sub_length': 'months subscribed, 0 if unknown',
  'channel.name': 'the channel being read',
  'channel.live': 'the channel is live (Lurkn)',
  'flags.action': 'sent with /me',
  'flags.highlighted': 'marked by a highlight, or names you',
  'flags.points_redeemed': 'highlighted with channel points',
  'flags.sub_message': 'a sub, resub, gift or upgrade',
  'flags.system_message': 'a channel event rather than a chatter',
  'flags.cheer_message': 'came with bits',
  'flags.reply': 'a reply to another message',
  'flags.deleted': 'removed by a moderator (Lurkn)',
  'flags.whisper': 'a whisper — never, in channel chat',
  'message.content': 'the text',
  'message.length': 'the length of the text',
};

export const OPERATORS =
  '&&  ||  !  ==  !=  <  <=  >  >=  contains  startswith  endswith  match  +  -  *  /  %  ( )  {a, b}';

// ─── Tokens ──────────────────────────────────────────────────────────────────

type Token =
  | { t: 'num'; value: number; at: number }
  | { t: 'str'; value: string; at: number }
  | { t: 'regex'; value: RegExp; at: number }
  | { t: 'ident'; value: string; at: number }
  | { t: 'op'; value: string; at: number }
  | { t: 'end'; at: number };

class FilterError extends Error {
  constructor(
    message: string,
    readonly at: number,
  ) {
    super(message);
  }
}

const WORD_OPS = new Set(['contains', 'startswith', 'endswith', 'match']);
const SYMBOLS = ['&&', '||', '==', '!=', '<=', '>=', '<', '>', '!', '+', '-', '*', '/', '%', '(', ')', '{', '}', ','];

/** Read a `"…"` string starting at the quote; returns the text and where it ended. */
function readString(source: string, quote: number): { text: string; end: number } {
  let text = '';
  let i = quote + 1;
  while (i < source.length) {
    const c = source[i];
    if (c === '\\' && i + 1 < source.length) {
      text += source[i + 1];
      i += 2;
      continue;
    }
    if (c === '"') return { text, end: i + 1 };
    text += c;
    i += 1;
  }
  throw new FilterError('This string is never closed with a "', quote);
}

function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < source.length) {
    const c = source[i] ?? '';

    if (/\s/.test(c)) {
      i += 1;
      continue;
    }

    // A regex is r"…" or ri"…"; checked before identifiers, which r and ri
    // would otherwise be.
    const regex = /^(ri|r)"/.exec(source.slice(i, i + 3));
    if (regex) {
      const flags = regex[1] === 'ri' ? 'i' : '';
      const quote = i + (regex[1]?.length ?? 1);
      const { text, end } = readString(source, quote);
      try {
        tokens.push({ t: 'regex', value: new RegExp(text, flags), at: i });
      } catch {
        throw new FilterError('This regex is not valid', i);
      }
      i = end;
      continue;
    }

    if (c === '"') {
      const { text, end } = readString(source, i);
      tokens.push({ t: 'str', value: text, at: i });
      i = end;
      continue;
    }

    const number = /^\d+(?:\.\d+)?/.exec(source.slice(i));
    if (number) {
      tokens.push({ t: 'num', value: Number(number[0]), at: i });
      i += number[0].length;
      continue;
    }

    const ident = /^[a-zA-Z_][a-zA-Z0-9_.]*/.exec(source.slice(i));
    if (ident) {
      const word = ident[0];
      const lower = word.toLowerCase();
      tokens.push(
        WORD_OPS.has(lower) ? { t: 'op', value: lower, at: i } : { t: 'ident', value: word, at: i },
      );
      i += word.length;
      continue;
    }

    const symbol = SYMBOLS.find((s) => source.startsWith(s, i));
    if (symbol) {
      tokens.push({ t: 'op', value: symbol, at: i });
      i += symbol.length;
      continue;
    }

    throw new FilterError(`"${c}" is not part of the filter language`, i);
  }

  tokens.push({ t: 'end', at: source.length });
  return tokens;
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/*
 * Precedence, loosest first:
 *   ||   &&   comparison (== != < <= > >= contains startswith endswith match)
 *   + -   * / %   ! and unary -
 * A comparison does not chain: `a == b == c` is an error rather than a guess.
 */
class Parser {
  #i = 0;
  constructor(private readonly tokens: Token[]) {}

  get #peek(): Token {
    return this.tokens[this.#i] ?? { t: 'end', at: 0 };
  }

  #take(): Token {
    const token = this.#peek;
    this.#i += 1;
    return token;
  }

  #isOp(...ops: string[]): boolean {
    const token = this.#peek;
    return token.t === 'op' && ops.includes(token.value);
  }

  parse(): Expr {
    const expr = this.#or();
    const rest = this.#peek;
    if (rest.t !== 'end') throw new FilterError('Expected && or || here', rest.at);
    return expr;
  }

  #binary(next: () => Expr, ops: string[]): Expr {
    let left = next();
    while (this.#isOp(...ops)) {
      const op = this.#take() as { value: BinaryOp };
      left = { kind: 'binary', op: op.value, left, right: next() };
    }
    return left;
  }

  #or = (): Expr => this.#binary(this.#and, ['||']);
  #and = (): Expr => this.#binary(this.#comparison, ['&&']);

  #comparison = (): Expr => {
    const left = this.#sum();
    const ops = ['==', '!=', '<', '<=', '>', '>=', 'contains', 'startswith', 'endswith', 'match'];
    if (!this.#isOp(...ops)) return left;

    const op = this.#take() as { value: BinaryOp };
    const right = this.#sum();
    if (this.#isOp(...ops)) {
      throw new FilterError('Comparisons do not chain; add brackets', this.#peek.at);
    }
    return { kind: 'binary', op: op.value, left, right };
  };

  #sum = (): Expr => this.#binary(this.#product, ['+', '-']);
  #product = (): Expr => this.#binary(this.#unary, ['*', '/', '%']);

  #unary = (): Expr => {
    if (this.#isOp('!')) {
      this.#take();
      return { kind: 'not', operand: this.#unary() };
    }
    if (this.#isOp('-')) {
      this.#take();
      return { kind: 'neg', operand: this.#unary() };
    }
    return this.#primary();
  };

  #primary = (): Expr => {
    const token = this.#take();
    switch (token.t) {
      case 'num':
      case 'str':
      case 'regex':
        return { kind: 'literal', value: token.value };

      case 'ident': {
        const lower = token.value.toLowerCase();
        if (lower === 'true') return { kind: 'literal', value: true };
        if (lower === 'false') return { kind: 'literal', value: false };
        if (!(lower in VARIABLES)) {
          throw new FilterError(`Lurkn knows no "${token.value}"`, token.at);
        }
        return { kind: 'var', name: lower };
      }

      case 'op':
        if (token.value === '(') {
          const inner = this.#or();
          if (!this.#isOp(')')) throw new FilterError('Expected ) here', this.#peek.at);
          this.#take();
          return inner;
        }
        if (token.value === '{') {
          const items: Expr[] = [];
          if (!this.#isOp('}')) {
            do {
              items.push(this.#or());
            } while (this.#isOp(',') && this.#take());
          }
          if (!this.#isOp('}')) throw new FilterError('Expected , or } here', this.#peek.at);
          this.#take();
          return { kind: 'list', items };
        }
        throw new FilterError(`Did not expect ${token.value} here`, token.at);

      case 'end':
        throw new FilterError('The filter stops before it is finished', token.at);
    }
  };
}

/** Parse a filter, or say what is wrong with it and where. */
export function parseFilter(source: string): Parsed {
  if (source.trim().length === 0) {
    return { ok: false, error: 'Empty filter', at: 0 };
  }
  try {
    return { ok: true, expr: new Parser(tokenize(source)).parse() };
  } catch (error) {
    if (error instanceof FilterError) return { ok: false, error: error.message, at: error.at };
    throw error;
  }
}

// ─── Evaluation ──────────────────────────────────────────────────────────────

export type Context = Record<string, Value>;

export function truthy(value: Value): boolean {
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === 'string') return value.length > 0;
  if (typeof value === 'number') return value !== 0;
  if (value instanceof RegExp) return true;
  return value;
}

function same(a: Value, b: Value): boolean {
  if (typeof a === 'string' && typeof b === 'string') return a.toLowerCase() === b.toLowerCase();
  if (typeof a === 'number' && typeof b === 'string') return a === Number(b);
  if (typeof a === 'string' && typeof b === 'number') return Number(a) === b;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, i) => same(item, b[i] ?? false));
  }
  return a === b;
}

function number(value: Value): number | null {
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value.trim() !== '' && !Number.isNaN(Number(value))) {
    return Number(value);
  }
  return null;
}

function compare(op: '<' | '<=' | '>' | '>=', a: Value, b: Value): boolean {
  const x = number(a);
  const y = number(b);
  if (x === null || y === null) return false;
  switch (op) {
    case '<':
      return x < y;
    case '<=':
      return x <= y;
    case '>':
      return x > y;
    case '>=':
      return x >= y;
  }
}

function arithmetic(op: '-' | '*' | '/' | '%', a: Value, b: Value): Value {
  const x = number(a);
  const y = number(b);
  if (x === null || y === null) return 0;
  switch (op) {
    case '-':
      return x - y;
    case '*':
      return x * y;
    // Division by zero is a filter typo, not a reason to hide everything.
    case '/':
      return y === 0 ? 0 : x / y;
    case '%':
      return y === 0 ? 0 : x % y;
  }
}

export function evaluate(expr: Expr, context: Context): Value {
  switch (expr.kind) {
    case 'literal':
      return expr.value;
    case 'var':
      return context[expr.name] ?? false;
    case 'not':
      return !truthy(evaluate(expr.operand, context));
    case 'neg':
      return -(number(evaluate(expr.operand, context)) ?? 0);
    case 'list':
      return expr.items.map((item) => evaluate(item, context));
    case 'binary':
      break;
  }

  const { op } = expr;

  // Short-circuit, so `flags.reply && …` costs nothing for most messages.
  if (op === '&&') return truthy(evaluate(expr.left, context)) && truthy(evaluate(expr.right, context));
  if (op === '||') return truthy(evaluate(expr.left, context)) || truthy(evaluate(expr.right, context));

  const left = evaluate(expr.left, context);
  const right = evaluate(expr.right, context);

  switch (op) {
    case '==':
      return same(left, right);
    case '!=':
      return !same(left, right);
    case '<':
    case '<=':
    case '>':
    case '>=':
      return compare(op, left, right);

    case 'contains':
      if (Array.isArray(left)) return left.some((item) => same(item, right));
      if (typeof left === 'string' && typeof right === 'string') {
        return left.toLowerCase().includes(right.toLowerCase());
      }
      return false;

    case 'startswith':
    case 'endswith': {
      if (Array.isArray(left)) {
        const end = op === 'startswith' ? left[0] : left[left.length - 1];
        return end !== undefined && same(end, right);
      }
      if (typeof left !== 'string' || typeof right !== 'string') return false;
      const [l, r] = [left.toLowerCase(), right.toLowerCase()];
      return op === 'startswith' ? l.startsWith(r) : l.endsWith(r);
    }

    case 'match':
      return typeof left === 'string' && right instanceof RegExp && right.test(left);

    case '+':
      if (typeof left === 'number' && typeof right === 'number') return left + right;
      if (Array.isArray(left) && Array.isArray(right)) return [...left, ...right];
      return `${String(left)}${String(right)}`;

    case '-':
    case '*':
    case '/':
    case '%':
      return arithmetic(op, left, right);
  }
}

/** Whether a message passes a parsed filter. */
export function passes(expr: Expr, context: Context): boolean {
  return truthy(evaluate(expr, context));
}

const SUB_EVENTS = new Set(['sub', 'resub', 'subGift', 'communityGift', 'upgrade']);

/** What a filter sees of one message. */
export function contextOf(
  message: ChatMessage,
  channel: { login: string | null; live: boolean },
): Context {
  const badges = message.author.badges;
  const sub = badges.find((b) => b.setId === 'subscriber' || b.setId === 'founder');
  const color = message.author.color;
  const event = message.event?.kind ?? null;

  return {
    'author.badges': badges.map((b) => b.setId),
    'author.color': color
      ? `#${[color.r, color.g, color.b].map((v) => v.toString(16).padStart(2, '0')).join('')}`
      : '',
    'author.name': message.author.displayName,
    'author.login': message.author.login,
    'author.no_color': color === null,
    'author.subbed': sub !== undefined,
    'author.sub_length': sub ? Number.parseInt(sub.info, 10) || 0 : 0,
    'channel.name': channel.login ?? '',
    'channel.live': channel.live,
    'flags.action': message.kind === 'action',
    'flags.highlighted': message.highlighted ?? false,
    'flags.points_redeemed': message.kind === 'highlight',
    'flags.sub_message': event !== null && SUB_EVENTS.has(event),
    'flags.system_message': message.kind === 'system' || (event !== null && event !== 'cheer'),
    'flags.cheer_message': event === 'cheer',
    'flags.reply': message.replyTo !== null,
    'flags.deleted': message.state.state === 'deleted',
    'flags.whisper': false,
    'message.content': message.rawText,
    'message.length': [...message.rawText].length,
  };
}
