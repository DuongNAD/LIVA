import registryJson from '../assets/avatar-animations.json';

export type AvatarAnimationKind = 'emotion' | 'state' | 'movement' | 'action';

export interface AvatarAnimationDefinition {
  id: number;
  key: string;
  kind: AvatarAnimationKind;
  clip: string | null;
  layer: 'face' | 'upper_body' | 'full_body' | 'movement';
  loop: boolean;
  durationMs: number;
  cooldownMs: number;
  modelSelectable: boolean;
  context: string;
}

export const AVATAR_ANIMATION_REGISTRY = registryJson as AvatarAnimationDefinition[];

const animationById = new Map(
  AVATAR_ANIMATION_REGISTRY.map((definition) => [definition.id, definition] as const)
);

export function getAvatarAnimation(id: number): AvatarAnimationDefinition | undefined {
  return animationById.get(id);
}
