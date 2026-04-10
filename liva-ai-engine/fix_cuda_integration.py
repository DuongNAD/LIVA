import os
import shutil
import subprocess

print("=============================================")
print(" FIXING CUDA + VISUAL STUDIO INTEGRATION")
print("=============================================")

# Locate the active CUDA toolkit directory (assuming default path or environment variable)
cuda_path = os.environ.get(
    "CUDA_PATH", r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.1"
)
if not os.path.exists(cuda_path):
    print(f"❌ Could not find CUDA at {cuda_path}")
    exit(1)

cuda_visual_studio_integration_dir = os.path.join(
    cuda_path, "extras", "visual_studio_integration", "MSBuildExtensions"
)

if not os.path.exists(cuda_visual_studio_integration_dir):
    print(
        f"❌ Could not find CUDA MSBuildExtensions at {cuda_visual_studio_integration_dir}"
    )
    exit(1)

# Locate the Visual Studio MSBuild directory
vs_path = r"C:\BuildTools"
if not os.path.exists(vs_path):
    vs_path = r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"

if not os.path.exists(vs_path):
    print(f"❌ Could not find Visual Studio Build Tools at {vs_path}")
    exit(1)

msbuild_extensions_dir = os.path.join(
    vs_path, "MSBuild", "Microsoft", "VC", "v170", "BuildCustomizations"
)

if not os.path.exists(msbuild_extensions_dir):
    # Try looking in program files if not in build tools
    vs_prog_path = r"C:\Program Files\Microsoft Visual Studio\2022\Community"
    if os.path.exists(vs_prog_path):
        msbuild_extensions_dir = os.path.join(
            vs_prog_path, "MSBuild", "Microsoft", "VC", "v170", "BuildCustomizations"
        )

    if not os.path.exists(msbuild_extensions_dir):
        print(f"❌ Could not find MSBuild Extensions dir at {msbuild_extensions_dir}")
        exit(1)

print(f"Found CUDA Extensions: {cuda_visual_studio_integration_dir}")
print(f"Found VS Extensions  : {msbuild_extensions_dir}")

# Copy the files
files_to_copy = [
    "CUDA 12.1.props",
    "CUDA 12.1.targets",
    "CUDA 12.1.xml",
    "Nvda.Build.CudaTasks.v12.1.dll",
]
copied = 0

for file_name in files_to_copy:
    src_file = os.path.join(cuda_visual_studio_integration_dir, file_name)
    dst_file = os.path.join(msbuild_extensions_dir, file_name)

    if os.path.exists(src_file):
        try:
            shutil.copy2(src_file, dst_file)
            print(f"✅ Copied {file_name}")
            copied += 1
        except Exception as e:
            print(f"❌ Failed to copy {file_name}: {e}")
    else:
        print(f"⚠️ Source file not found: {src_file}")

print(f"\nIntegration fix complete. Copied {copied} files.")

if copied > 0:
    print("\nAttempting to run the python installer again...")
    subprocess.run([r".\venv\Scripts\python.exe", "install_cuda.py"], shell=True)
