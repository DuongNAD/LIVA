export const DEFAULT_WIDGET_MODEL = {
  filename: "models/vrm/default_avatar/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx",
  type: "3d",
  format: "fbx",
};

export const resolveEngineFromConfig = (config: any) => {
  const avatarMode = config?.ui?.avatarMode ?? config?.avatarMode ?? config?.avatar?.engineMode;
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;

  if (avatarMode === '2D' || avatarMode === '3D') {
    return avatarMode;
  }

  if (activeModel?.type === '3d' || activeModel?.format === 'vrm' || activeModel?.format === 'fbx') return '3D';
  if (activeModel?.type === '2d') return '2D';

  return '3D';
};

export const normalizeModelConfig = (config: any) => {
  const activeModel = config?.ui?.activeModel ?? config?.activeModel ?? config?.avatar?.activeModel;
  const avatar = config?.avatar ?? {};

  if (activeModel?.filename) return activeModel;

  const candidate = avatar.vrmModel || avatar.live2dModel;
  if (candidate) {
    const lower = String(candidate).toLowerCase();
    return {
      filename: candidate,
      type: lower.includes('/live2d/') ? '2d' : '3d',
      format: lower.endsWith('.fbx') ? 'fbx' : lower.endsWith('.vrm') ? 'vrm' : 'json',
    };
  }

  return DEFAULT_WIDGET_MODEL;
};
