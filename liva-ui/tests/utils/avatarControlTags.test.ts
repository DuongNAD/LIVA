import { describe, expect, it } from 'vitest';
import { AvatarControlTagStream, stripAvatarControlTags } from '../../src/utils/avatarControlTags';

describe('AvatarControlTagStream', () => {
  it('keeps a split action tag out of visible text until the tag is complete', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('[wa')).toEqual({ text: '', controls: [] });
    expect(stream.push('ve]Xin chào')).toEqual({
      text: 'Xin chào',
      controls: [{ type: 'action', value: 'wave' }],
    });
  });

  it('extracts emotion and action tags independently', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('[happy] [jump] Tuyệt quá!')).toEqual({
      text: 'Tuyệt quá!',
      controls: [
        { type: 'emotion', value: 'happy' },
        { type: 'action', value: 'jump' },
      ],
    });
  });

  it('extracts a stable numeric animation id even when the tag is split', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('[anim:2')).toEqual({ text: '', controls: [] });
    expect(stream.push('01]Xin chào')).toEqual({
      text: 'Xin chào',
      controls: [{ type: 'animation', value: 201 }],
    });
  });

  it('strips an unknown numeric animation id without executing it or leaking it', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Chào bạn. [anim:999999] Tiếp tục.')).toEqual({
      text: 'Chào bạn.  Tiếp tục.',
      controls: [],
    });
  });

  it('silently removes an unknown leading control tag', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('[dance] Xin chào')).toEqual({
      text: 'Xin chào',
      controls: [],
    });
  });

  it('leaves bracketed text untouched after visible text has started', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Kết quả [2 + 2] là 4')).toEqual({
      text: 'Kết quả [2 + 2] là 4',
      controls: [],
    });
  });

  // ── U26: tag giữa câu ────────────────────────────────────────────────────

  it('bắt được tag nằm giữa câu, không chỉ ở đầu lượt', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Chào bạn. [happy] Vui quá!')).toEqual({
      text: 'Chào bạn.  Vui quá!',
      controls: [{ type: 'emotion', value: 'happy' }],
    });
  });

  it('bắt nhiều tag giữa câu theo đúng thứ tự xuất hiện', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Xong rồi[nod], nhưng hỏng[sad] mất.')).toEqual({
      text: 'Xong rồi, nhưng hỏng mất.',
      controls: [
        { type: 'action', value: 'nod' },
        { type: 'emotion', value: 'sad' },
      ],
    });
  });

  it('giữ lại tag LẠ ở giữa câu — khác hẳn cách xử ở tiền tố', () => {
    const stream = new AvatarControlTagStream();

    // Ở tiền tố `[dance]` bị nuốt (test bên trên). Ở giữa câu thì không, nếu
    // không mọi ngoặc vuông hợp lệ trong văn xuôi đều biến mất.
    expect(stream.push('Nhạc nền [dance mix] rất hay.')).toEqual({
      text: 'Nhạc nền [dance mix] rất hay.',
      controls: [],
    });
  });

  it('ghép đúng tag giữa câu bị cắt đôi giữa hai chunk', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Chào bạn. [wa')).toEqual({
      text: 'Chào bạn. ',
      controls: [],
    });
    expect(stream.push('ve] Hẹn gặp lại.')).toEqual({
      text: ' Hẹn gặp lại.',
      controls: [{ type: 'action', value: 'wave' }],
    });
  });

  it('không nghẽn luồng vì một ngoặc không thể thành tag', () => {
    const stream = new AvatarControlTagStream();

    // `2 + 2` không thể lớn lên thành tag nào ⇒ nhả ngay, TTS không phải đợi
    // dấu `]` ở chunk sau. Khác với `[wa` ở test ngay trên.
    expect(stream.push('Kết quả [2 + 2')).toEqual({
      text: 'Kết quả [2 + 2',
      controls: [],
    });
    expect(stream.push('] là 4.')).toEqual({ text: '] là 4.', controls: [] });
  });

  it('bỏ tag đang dở ở cuối luồng thay vì đọc nó lên', () => {
    const stream = new AvatarControlTagStream();

    expect(stream.push('Chào bạn. [ha')).toEqual({
      text: 'Chào bạn. ',
      controls: [],
    });
    expect(stream.flush()).toBe('');
  });

  it('reset() đưa bộ đọc về lại chế độ tiền tố', () => {
    const stream = new AvatarControlTagStream();

    stream.push('Chào bạn.');
    stream.reset();

    // Ở tiền tố khoảng trắng sau tag bị trim; ở giữa câu thì không (xem các ca
    // "Chào bạn.  Vui quá!" bên trên). Khác biệt này có từ trước, giữ nguyên.
    expect(stream.push('[angry] Lượt mới.')).toEqual({
      text: 'Lượt mới.',
      controls: [{ type: 'emotion', value: 'angry' }],
    });
  });
});

describe('stripAvatarControlTags', () => {
  it('cleans final response text without returning controls', () => {
    expect(stripAvatarControlTags('[relaxed][nod] Được, mình hiểu.')).toBe('Được, mình hiểu.');
  });

  it('drops an unfinished leading tag instead of leaking it', () => {
    expect(stripAvatarControlTags('[wa')).toBe('');
  });

  it('strips known and unknown numeric animation control tags', () => {
    expect(stripAvatarControlTags('[anim:201]Xin chào[anim:999999] bạn.')).toBe('Xin chào bạn.');
  });

  /**
   * Bảng ca kiểm dùng CHUNG với `bang_ca_kiem_chung_voi_ban_typescript` trong
   * `liva-native-core/src/tts/avatar_control.rs`. Hai bản cài đặt phải cho ra
   * cùng một văn bản còn lại — lệch là TTS đọc lên một tag mà UI đã nuốt, hoặc
   * ngược lại. Sửa bảng này thì phải sửa cả bảng bên Rust.
   */
  it('khớp từng ca với bản lọc phía Rust', () => {
    const cases: ReadonlyArray<readonly [string, string]> = [
      ['[happy] Xin chào.', 'Xin chào.'],
      ['[happy] [jump] [dance] Tuyệt quá!', 'Tuyệt quá!'],
      ['Kết quả [2 + 2] là 4.', 'Kết quả [2 + 2] là 4.'],
      ['Chào bạn. [happy] Vui quá!', 'Chào bạn.  Vui quá!'],
      ['Xong rồi[nod], nhưng hỏng[sad] mất.', 'Xong rồi, nhưng hỏng mất.'],
      ['Nhạc nền [dance mix] rất hay.', 'Nhạc nền [dance mix] rất hay.'],
      ['[come_closer]Lại đây[step_back] rồi lùi.', 'Lại đây rồi lùi.'],
      ['Không có tag nào cả.', 'Không có tag nào cả.'],
    ];

    for (const [input, expected] of cases) {
      expect(stripAvatarControlTags(input), `đầu vào: ${input}`).toBe(expected);
    }
  });
});
