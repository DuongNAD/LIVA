"""
Fix CUDA compatibility for RTX 50 series (Blackwell sm_120).
RTX 5060 Ti requires PyTorch with CUDA 12.6+ or nightly builds.

This script checks GPU compatibility and recommends fixes.
"""

import subprocess
import sys

def check_gpu():
    try:
        import torch
        print(f"PyTorch: {torch.__version__}")
        
        if torch.cuda.is_available():
            gpu_name = torch.cuda.get_device_name(0)
            print(f"GPU: {gpu_name}")
            
            props = torch.cuda.get_device_properties(0)
            print(f"Compute Capability: sm_{props.major}{props.minor}")
            
            # Check if Blackwell (sm_120)
            if props.major >= 10:
                print("\n⚠️ RTX 50 series (Blackwell) detected!")
                print("PyTorch 2.5.x with CUDA 12.1 does NOT support sm_120.")
                print("\nOptions:")
                print("1. Use PyTorch nightly: pip install --pre torch --index-url https://download.pytorch.org/whl/nightly/cu126")
                print("2. Use CPU fallback for Whisper (llama.cpp still uses GPU)")
                print("3. Wait for official PyTorch 2.6 with Blackwell support")
            else:
                print("✓ GPU should work with current PyTorch")
        else:
            print("CUDA not available - using CPU fallback")
            
    except ImportError:
        print("PyTorch not installed")

if __name__ == "__main__":
    check_gpu()
