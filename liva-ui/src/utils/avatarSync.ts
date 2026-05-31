/**
 * avatarSync.ts — Shared helpers for Dashboard ↔ Widget avatar config SSOT
 */

export type EnginePreference = 'auto' | '2D' | '3D';
export type ModelFormat = 'vrm' | 'fbx' | 'live2d';

export interface AvatarModelInfo {
  name: string;
  filename: string;
  size: string;
  isActive: boolean;
  type: '2d' | '3d';
  format?: ModelFormat;
}

export function normalizeEngineMode(raw: unknown): EnginePreference {
  const v = String(raw ?? 'auto').toLowerCase();
  if (v === '2d') return '2D';
  if (v === '3d') return '3D';
  return 'auto';
}

function normalizePath(p: string): string {
  return p.replace(/\\/g, '/').toLowerCase();
}

/** Resolve the single authoritative active model path from config (SSOT priority) */
export function getActiveModelKey(config: Record<string, unknown> | null | undefined): string | null {
  if (!config) return null;
  const ui = config.ui as Record<string, unknown> | undefined;
  const uiModel = ui?.activeModel as Record<string, unknown> | undefined;
  const avatar = config.avatar as Record<string, unknown> | undefined;

  if (typeof uiModel?.filename === 'string' && uiModel.filename) {
    return normalizePath(uiModel.filename);
  }
  if (typeof avatar?.vrmModel === 'string' && avatar.vrmModel) {
    return normalizePath(avatar.vrmModel);
  }
  if (typeof avatar?.live2dModel === 'string' && avatar.live2dModel) {
    return normalizePath(avatar.live2dModel);
  }
  if (typeof avatar?.activeModel === 'string' && avatar.activeModel) {
    return normalizePath(avatar.activeModel);
  }
  return null;
}

/** Match active model flags from liva-config against a gallery entry */
export function isModelActive(model: AvatarModelInfo, config: Record<string, unknown> | null | undefined): boolean {
  const activeKey = getActiveModelKey(config);
  if (!activeKey) return false;

  const modelKeys = [
    normalizePath(model.filename),
    normalizePath(`models/vrm/${model.filename}`),
    normalizePath(`models/live2d/${model.filename}`),
  ];

  return modelKeys.some((k) => activeKey === k);
}

/** Build a partial config patch that keeps Widget + Gateway SSOT aligned */
export function buildAvatarConfigPatch(
  model: AvatarModelInfo,
  engine: EnginePreference,
): Record<string, unknown> {
  const basePath = model.type === '3d' ? `models/vrm/${model.filename}` : `models/live2d/${model.filename}`;

  return {
    avatar: {
      engineMode: engine,
      activeModel: model.filename,
      activeType: model.type,
      activeFormat: model.format ?? null,
      ...(model.type === '3d'
        ? { vrmModel: basePath }
        : { live2dModel: basePath }),
    },
    ui: {
      avatarMode: engine,
      activeModel: {
        filename: basePath,
        type: model.type,
        format: model.format ?? (model.type === '2d' ? 'live2d' : 'fbx'),
      },
    },
  };
}

export function applyActiveFlags(
  models: AvatarModelInfo[],
  config: Record<string, unknown> | null | undefined,
): AvatarModelInfo[] {
  return models.map((m) => ({ ...m, isActive: isModelActive(m, config) }));
}
