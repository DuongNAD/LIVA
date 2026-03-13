import subprocess
import os
import sys

def run_cmd(cmd):
    print(f"Running: {cmd}")
    result = subprocess.run(cmd, shell=True)
    return result.returncode

print("=============================================")
print(" LIVA AI - CUDA SETUP FOR RTX 5060 Ti")
print("=============================================")

# 1. Check & Install Build Tools
vcvars_paths = [
    r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat",
    r"C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
]

has_build_tools = False
for path in vcvars_paths:
    if os.path.exists(path):
        has_build_tools = True
        break

if not has_build_tools:
    print("[1/4] Downloading VS Build Tools...")
    exe_path = os.path.join(os.environ["TEMP"], "vs_buildtools.exe")
    run_cmd(f"powershell -c \"Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vs_buildtools.exe' -OutFile '{exe_path}'\"")
    
    print("Installing silently (5-10 mins). Please wait...")
    install_cmd = f"\"{exe_path}\" --quiet --wait --norestart --nocache --installPath C:\\BuildTools --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    run_cmd(install_cmd)

print("\n[2/4] Cleaning up old llama-cpp-python...")
run_cmd(f"\"{sys.executable}\" -m pip uninstall -y llama-cpp-python")

print("\n[3/4] Compiling Llama-cpp with CUDA 12.1...")
os.environ["CMAKE_ARGS"] = "-DGGML_CUDA=on"
os.environ["FORCE_CMAKE"] = "1"

build_cmd = ""
# Find vcvars64 again
for path in vcvars_paths:
    if os.path.exists(path):
        build_cmd = f"\"{path}\" && "
        break

build_cmd += f"\"{sys.executable}\" -m pip install llama-cpp-python --no-cache-dir --upgrade --force-reinstall"
run_cmd(build_cmd)

print("\n[4/4] DONE! You can now run Gateway and AI Engine.")
