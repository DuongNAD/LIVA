declare module "three" {
  export class Object3D {
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
    scale: {
      multiplyScalar: (scale: number) => void;
    };
    skeleton?: { dispose: () => void };
    traverse: (callback: (object: Object3D) => void) => void;
  }

  export class Scene extends Object3D {
    add: (...objects: unknown[]) => void;
    remove: (...objects: unknown[]) => void;
  }

  export class PerspectiveCamera extends Object3D {
    constructor(fov: number, aspect: number, near: number, far: number);
    aspect: number;
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
  }

  export class Box3 {
    min: { y: number };
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
