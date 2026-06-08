import os
import platform
import logging
import gc

# [CRITICAL UPDATE]: Bắt buộc phải set trước khi import torch
# Để các phép toán chưa được MPS hỗ trợ tự động chạy bằng CPU thay vì gây crash.
os.environ["PYTORCH_ENABLE_MPS_FALLBACK"] = "1"

import torch

logging.basicConfig(format='%(asctime)s - %(name)s - %(levelname)s - %(message)s', level=logging.INFO)
logger = logging.getLogger("LivaCore.HardwareAllocator")

class HardwareAllocator:
    @staticmethod
    def get_optimal_device() -> torch.device:
        if platform.system() == "Darwin" and platform.machine() == "arm64":
            if getattr(torch.backends, "mps", None) and torch.backends.mps.is_available():
                logger.info("🚀 [Apple Silicon] Metal Performance Shaders (MPS) Activated.")
                return torch.device("mps")
            logger.warning("⚠️ [Apple Silicon] MPS unavailable. Fallback to CPU.")
            return torch.device("cpu")
            
        elif torch.cuda.is_available():
            logger.info(f"🟢 [NVIDIA] CUDA Activated: {torch.cuda.get_device_name(0)}")
            return torch.device("cuda")
            
        logger.warning("🔴 [Fallback] No AI accelerator detected. Running on CPU.")
        return torch.device("cpu")

    @staticmethod
    def flush_memory(device: torch.device, force_gc: bool = False):
        """
        Dọn dẹp bộ nhớ. 
        Tham số force_gc: Chỉ gọi gc.collect() khi thật sự cần thiết (ví dụ: đổi model), 
        không nên gọi liên tục trong streaming loop để tránh micro-stuttering.
        """
        if force_gc:
            gc.collect()
            
        if device.type == "mps":
            torch.mps.empty_cache()
            # logger.debug("🧹 MPS Cache cleared.")
        elif device.type == "cuda":
            torch.cuda.empty_cache()
            # logger.debug("🧹 CUDA Cache cleared.")
