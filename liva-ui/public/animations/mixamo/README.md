# Mixamo animation assets

Copy the six animation-only FBX files into this directory:

- `idle.fbx`
- `walk.fbx` — export the In Place variant
- `run.fbx` — export the In Place variant
- `jump.fbx`
- `wave.fbx`
- `thinking.fbx`

Download settings: FBX Binary, Without Skin, 30 FPS, Keyframe Reduction None.
The runtime retargets `mixamorig:*` rotations to the normalized VRM humanoid rig. Missing files are optional and retain the procedural fallback.
