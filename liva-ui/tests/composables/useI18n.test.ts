/**
 * useI18n.test.ts — Unit Tests
 * ==============================
 * Tests the i18n translation composable:
 *   - Default language fallback
 *   - vi-VN / en-US translation correctness
 *   - Unknown key passthrough
 *   - Param interpolation ({count}, {status}, etc.)
 *   - Unknown locale fallback to vi-VN
 *
 * Because useI18n reads useGateway().userProfile (module-level singleton),
 * we must vi.resetModules() + dynamic import() for tests that switch language.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

// ─── Helpers ───
async function loadI18nWithLang(lang: string | undefined) {
  vi.resetModules();

  vi.doMock('../../src/composables/useGateway', () => ({
    useGateway: vi.fn().mockReturnValue({
      userProfile: { value: lang !== undefined ? { language: lang } : null },
      isConnected: { value: true },
    }),
  }));

  const mod = await import('../../src/composables/useI18n');
  return mod.useI18n();
}

// ─── Tests ───
describe('useI18n — Language Defaults', () => {
  it('should default to vi-VN when userProfile is null (no language set)', async () => {
    const { currentLang } = await loadI18nWithLang(undefined);
    expect(currentLang.value).toBe('vi-VN');
  });

  it('should use en-US when userProfile.language is en-US', async () => {
    const { currentLang } = await loadI18nWithLang('en-US');
    expect(currentLang.value).toBe('en-US');
  });

  it('should use vi-VN when userProfile.language is vi-VN', async () => {
    const { currentLang } = await loadI18nWithLang('vi-VN');
    expect(currentLang.value).toBe('vi-VN');
  });
});

describe('useI18n — Translation vi-VN', () => {
  it('should translate keys correctly in vi-VN', async () => {
    const { t } = await loadI18nWithLang('vi-VN');

    expect(t('nav_settings')).toBe('Cài đặt hệ thống');
    expect(t('connected')).toBe('Đã kết nối');
    expect(t('lang_code')).toBe('vi-VN');
    expect(t('nav_avatar')).toBe('Avatar');
  });
});

describe('useI18n — Translation en-US', () => {
  it('should translate keys correctly in en-US', async () => {
    const { t } = await loadI18nWithLang('en-US');

    expect(t('nav_settings')).toBe('System Settings');
    expect(t('connected')).toBe('Connected');
    expect(t('lang_code')).toBe('en-US');
    expect(t('nav_avatar')).toBe('Avatar');
  });
});

describe('useI18n — Unknown Keys', () => {
  it('should return the key itself for unknown translation keys', async () => {
    const { t } = await loadI18nWithLang('vi-VN');

    expect(t('this_key_does_not_exist')).toBe('this_key_does_not_exist');
    expect(t('')).toBe('');
    expect(t('random_missing_key_xyz')).toBe('random_missing_key_xyz');
  });
});

describe('useI18n — Param Interpolation', () => {
  it('should interpolate {params} in translations', async () => {
    const { t } = await loadI18nWithLang('vi-VN');

    const result = t('sys_service_health', { count: 5 });
    expect(result).toBe('TRẠNG THÁI DỊCH VỤ (5 ONLINE)');
  });

  it('should interpolate {params} in en-US translations', async () => {
    const { t } = await loadI18nWithLang('en-US');

    const result = t('sys_service_health', { count: 3 });
    expect(result).toBe('SERVICE HEALTH (3 ONLINE)');
  });

  it('should interpolate multiple params', async () => {
    const { t } = await loadI18nWithLang('vi-VN');

    // tm_empty_filter has {status}
    const result = t('tm_empty_filter', { status: 'Đang chạy' });
    expect(result).toBe('Không có task nào trong trạng thái "Đang chạy".');
  });

  it('should leave text unchanged when no params are provided', async () => {
    const { t } = await loadI18nWithLang('en-US');

    // The raw string still has {count} placeholder
    const result = t('sys_service_health');
    expect(result).toBe('SERVICE HEALTH ({count} ONLINE)');
  });
});

describe('useI18n — Unknown Locale Fallback', () => {
  it('should fallback to vi-VN for unknown locale', async () => {
    const { t, currentLang } = await loadI18nWithLang('ja-JP');

    // currentLang is 'ja-JP' but dictionary doesn't exist, so t() should fallback to vi-VN
    expect(currentLang.value).toBe('ja-JP');
    expect(t('nav_settings')).toBe('Cài đặt hệ thống');
    expect(t('connected')).toBe('Đã kết nối');
    expect(t('lang_code')).toBe('vi-VN');
  });
});
