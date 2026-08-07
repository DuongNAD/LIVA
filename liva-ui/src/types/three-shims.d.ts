declare module "three" {
  export class Object3D {
    name: string;
    parent: Object3D | null;
    animations: unknown[];
    geometry?: { dispose: () => void };
    isMesh?: boolean;
    material?: unknown;
    position: {
      x: number;
      y: number;
      z: number;
      set: (x: number, y: number, z: number) => void;
      sub: (vector: Vector3) => void;
    };
    rotation: {
      x: number;
      y: number;
      z: number;
    };
    quaternion: Quaternion;
    scale: {
      x: number;
      y: number;
      z: number;
      set: (x: number, y: number, z: number) => void;
      multiplyScalar: (scale: number) => void;
    };
    visible: boolean;
    skeleton?: { dispose: () => void };
    add: (...objects: unknown[]) => void;
    remove: (...objects: unknown[]) => void;
    updateMatrixWorld: (force?: boolean) => void;
    updateWorldMatrix: (updateParents: boolean, updateChildren: boolean) => void;
    getWorldPosition: (target: Vector3) => Vector3;
    getWorldQuaternion: (target: Quaternion) => Quaternion;
    worldToLocal: (vector: Vector3) => Vector3;
    traverse: (callback: (object: Object3D) => void) => void;
  }

  export class Scene extends Object3D {
    add: (...objects: unknown[]) => void;
    remove: (...objects: unknown[]) => void;
  }

  export class PerspectiveCamera extends Object3D {
    constructor(fov: number, aspect: number, near: number, far: number);
    aspect: number;
    /** Góc mở dọc, tính bằng độ — cần để suy ra khung nhìn tại mặt phẳng nhân vật đứng */
    fov: number;
    lookAt: (x: number, y: number, z: number) => void;
    updateProjectionMatrix: () => void;
  }

  export class WebGLRenderer {
    constructor(parameters: { canvas: HTMLCanvasElement; alpha?: boolean; antialias?: boolean });
    dispose: () => void;
    forceContextLoss: () => void;
    render: (scene: Scene, camera: PerspectiveCamera) => void;
    setClearColor: (color: number, alpha?: number) => void;
    setPixelRatio: (ratio: number) => void;
    setSize: (width: number, height: number) => void;
  }

  export class Clock {
    getDelta: () => number;
  }

  export class Vector3 {
    constructor(x?: number, y?: number, z?: number);
    x: number;
    y: number;
    z: number;
    set: (x: number, y: number, z: number) => this;
    add: (vector: Vector3) => this;
    sub: (vector: Vector3) => this;
    clone: () => Vector3;
    /** Chiếu điểm world → toạ độ clip [-1,1] của camera (dùng để suy ra hộp bao trên màn hình) */
    project: (camera: PerspectiveCamera) => this;
  }

  export class Quaternion {
    constructor(x?: number, y?: number, z?: number, w?: number);
    x: number;
    y: number;
    z: number;
    w: number;
  }

  export class Box3 {
    min: { x: number; y: number; z: number };
    max: { x: number; y: number; z: number };
    getCenter: (target: Vector3) => Vector3;
    getSize: (target: Vector3) => Vector3;
    setFromObject: (object: Object3D) => this;
  }

  export class Group extends Object3D {}

  export class AnimationMixer {
    constructor(root: Object3D);
    clipAction: (clip: unknown) => { play: () => void };
    stopAllAction: () => void;
    update: (delta: number) => void;
  }

  export class AmbientLight extends Object3D {
    constructor(color: number, intensity?: number);
  }

  export class HemisphereLight extends Object3D {
    constructor(skyColor: number, groundColor: number, intensity?: number);
  }

  export class DirectionalLight extends Object3D {
    constructor(color: number, intensity?: number);
  }

  export class Mesh extends Object3D {
    geometry: { dispose: () => void };
    material: Material | Material[];
  }

  export class Material {
    dispose: () => void;
  }
}

declare module "three/examples/jsm/loaders/GLTFLoader.js" {
  import type { Object3D } from "three";

  export class GLTFLoader {
    register: (callback: (parser: unknown) => unknown) => void;
    load: (
      path: string,
      onLoad: (gltf: { userData: Record<string, unknown>; scene: Object3D }) => void,
      onProgress?: (event: ProgressEvent<EventTarget>) => void,
      onError?: (error: unknown) => void,
    ) => void;
  }
}

declare module "three/examples/jsm/loaders/FBXLoader.js" {
  import type { Group } from "three";

  export class FBXLoader {
    load: (
      path: string,
      onLoad: (fbx: Group) => void,
      onProgress?: (event: ProgressEvent<EventTarget>) => void,
      onError?: (error: unknown) => void,
    ) => void;
  }
}
