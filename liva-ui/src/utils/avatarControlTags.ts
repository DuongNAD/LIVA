export const AVATAR_EMOTIONS = [
  'happy',
  'sad',
  'angry',
  'surprised',
  'neutral',
  'relaxed',
] as const;

export const AVATAR_ACTIONS = ['wave', 'nod', 'jump', 'come_closer', 'step_back'] as const;

export type AvatarEmotion = (typeof AVATAR_EMOTIONS)[number];
export type AvatarAction = (typeof AVATAR_ACTIONS)[number];

export type AvatarControl =
  | { type: 'emotion'; value: AvatarEmotion }
  | { type: 'action'; value: AvatarAction }
  | { type: 'animation'; value: number };

export interface AvatarControlChunk {
  text: string;
  controls: AvatarControl[];
}

const emotionTags = new Set<string>(AVATAR_EMOTIONS);
const actionTags = new Set<string>(AVATAR_ACTIONS);

/**
 * Every tag the LLM may emit. Must stay identical to `AVATAR_CONTROL_TAGS` in
 * `liva-native-core/src/tts/avatar_control.rs` — one name out of step and the
 * UI swallows a tag the TTS still reads aloud, or the reverse.
 */
const ALL_TAGS: readonly string[] = [...AVATAR_EMOTIONS, ...AVATAR_ACTIONS];

function toControl(tag: string): AvatarControl | null {
  const animationMatch = /^anim:(\d{1,6})$/.exec(tag);
  if (animationMatch) {
    const id = Number(animationMatch[1]);
    if (getAvatarAnimation(id)?.modelSelectable) {
      return { type: 'animation', value: id };
    }
    return null;
  }
  if (emotionTags.has(tag)) {
    return { type: 'emotion', value: tag as AvatarEmotion };
  }
  if (actionTags.has(tag)) {
    return { type: 'action', value: tag as AvatarAction };
  }
  return null;
}

/** Could `partial` still grow into a real tag? Bounds how long we hold text back. */
function isViableTagPrefix(partial: string): boolean {
  return (
    ALL_TAGS.some((tag) => tag.startsWith(partial)) ||
    'anim:'.startsWith(partial) ||
    /^anim:\d{0,6}$/.test(partial)
  );
}

function isAnimationTagSyntax(tag: string): boolean {
  return /^anim:\d{1,6}$/.test(tag);
}

/**
 * Parses avatar control tags out of one streamed assistant response.
 *
 * Two modes, deliberately different:
 *
 * - **Prefix** (before any visible text): *every* bracketed group is swallowed,
 *   known tag or not. A hallucinated `[dance]` must never reach the TTS, and at
 *   the head of a reply there is no legitimate bracket to protect.
 * - **Body** (after visible text starts): only brackets whose content matches a
 *   known tag are swallowed. This is what lets emotion change mid-reply instead
 *   of being fixed by the first tag — while `Kết quả [2 + 2] là 4.` still reads
 *   out intact, because `2 + 2` is not a tag.
 */
export class AvatarControlTagStream {
  private pending = '';
  private readingControlPrefix = true;

  push(chunk: string): AvatarControlChunk {
    this.pending += chunk;
    const controls: AvatarControl[] = [];

    while (this.readingControlPrefix) {
      this.pending = this.pending.replace(/^\s+/, '');
      if (this.pending.length === 0) {
        return { text: '', controls };
      }

      if (!this.pending.startsWith('[')) {
        this.readingControlPrefix = false;
        break;
      }

      const closingBracket = this.pending.indexOf(']');
      if (closingBracket === -1) {
        return { text: '', controls };
      }

      const control = toControl(this.pending.slice(1, closingBracket));
      if (control) controls.push(control);
      this.pending = this.pending.slice(closingBracket + 1);
    }

    return { text: this.drainBody(controls), controls };
  }

  /** Whitelist-only tag stripping over the visible part of the reply. */
  private drainBody(controls: AvatarControl[]): string {
    let out = '';

    for (;;) {
      const open = this.pending.indexOf('[');
      if (open === -1) {
        out += this.pending;
        this.pending = '';
        return out;
      }

      const closingBracket = this.pending.indexOf(']', open + 1);
      if (closingBracket === -1) {
        // No `]` yet. Hold the tail back only while it could still become a
        // tag, so a stream split as `…[ha` + `ppy] …` still resolves — but
        // `Kết quả [2 + 2` goes straight out instead of making the TTS wait on
        // a bracket that will never close.
        if (isViableTagPrefix(this.pending.slice(open + 1))) {
          out += this.pending.slice(0, open);
          this.pending = this.pending.slice(open);
        } else {
          out += this.pending;
          this.pending = '';
        }
        return out;
      }

      const control = toControl(this.pending.slice(open + 1, closingBracket));
      out += this.pending.slice(0, open);
      if (control) {
        controls.push(control);
      } else if (isAnimationTagSyntax(this.pending.slice(open + 1, closingBracket))) {
        // Numeric control syntax is protocol data. Unknown IDs fail closed: no action,
        // no visible/TTS leakage, and no chance to execute an unregistered animation.
      } else {
        // A real bracket in the prose — hand it back untouched.
        out += this.pending.slice(open, closingBracket + 1);
      }
      this.pending = this.pending.slice(closingBracket + 1);
    }
  }

  /**
   * End of stream. Whatever is still pending is either an unterminated prefix
   * bracket or a held-back viable tag prefix — never ordinary text, because
   * drainBody() releases anything that cannot become a tag. So this fails
   * closed: a truncated `[ha` is dropped rather than spoken as "ha".
   */
  flush(): string {
    this.pending = '';
    return '';
  }

  reset(): void {
    this.pending = '';
    this.readingControlPrefix = true;
  }
}

export function stripAvatarControlTags(text: string): string {
  const stream = new AvatarControlTagStream();
  const parsed = stream.push(text);
  return parsed.text + stream.flush();
}
import { getAvatarAnimation } from './avatarAnimationRegistry';
