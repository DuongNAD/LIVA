"""
VRAM Manager - Quản lý bộ nhớ GPU

Features:
- asyncio.Lock() để xếp hàng các request GPU
- Kiểm tra VRAM trước khi allocate
- Giải phóng VRAM triệt để sau mỗi step
- Timeout để chờ VRAM
"""

import asyncio
import gc
import time
import torch
from typing import Optional

# Singleton instance
_vram_manager: Optional['VRAMManager'] = None


def get_vram_manager() -> 'VRAMManager':
    """Get singleton VRAM manager instance"""
    global _vram_manager
    if _vram_manager is None:
        _vram_manager = VRAMManager()
    return _vram_manager


class VRAMManager:
    """
    GPU Memory Manager
    
    Features:
    - Global asyncio.Lock() để chống race condition
    - Kiểm tra VRAM trước khi load model
    - Giải phóng VRAM triệt để (gc.collect + empty_cache)
    - Chờ VRAM với timeout
    
    Usage:
        async with VRAMManager.gpu_lock:
            # Tất cả GPU operations ở đây
            model = load_my_model()
            result = model(input)
            # VRAMManager.release() được gọi tự động
    """
    
    # Lock toàn cục - chống race condition khi nhiều request
    gpu_lock: asyncio.Lock = asyncio.Lock()
    
    # Ngưỡng VRAM cho từng model (MB)
    VRAM_REQUIREMENTS = {
        "deepfilternet": 100,
        "silero_vad": 50,
        "whisper_tiny": 200,
        "whisper_small": 400,
        "whisper_medium": 800,
        "whisper_large": 1200,
        "whisper_large_v3_turbo": 1400,
        "gpt_sovits": 1500,
        "speechbrain": 200,
        "xtts": 1800,
    }
    
    # Ngưỡng an toàn
    DEFAULT_SAFETY_MARGIN = 1.3  # 30% buffer
    TIMEOUT_SEC = 300  # 5 phút chờ VRAM
    
    def __init__(self):
        self._debug = True
        self._initialized = torch.cuda.is_available()
        
        if self._initialized:
            print(f"    💾 VRAM Manager initialized")
            print(f"    💾 Total VRAM: {self.get_total_vram_mb():.0f}MB")
    
    @property
    def is_cuda_available(self) -> bool:
        """Check if CUDA is available"""
        return self._initialized and torch.cuda.is_available()
    
    @staticmethod
    def get_total_vram_mb() -> float:
        """Lấy tổng VRAM (MB)"""
        if not torch.cuda.is_available():
            return 0
        return torch.cuda.get_device_properties(0).total_memory / 1024**2
    
    @staticmethod
    def get_allocated_vram_mb() -> float:
        """Lấy VRAM đã sử dụng (MB)"""
        if not torch.cuda.is_available():
            return 0
        return torch.cuda.memory_allocated() / 1024**2
    
    @staticmethod
    def get_free_vram_mb() -> float:
        """Lấy VRAM còn trống (MB)"""
        if not torch.cuda.is_available():
            return 0
        total = VRAMManager.get_total_vram_mb()
        allocated = VRAMManager.get_allocated_vram_mb()
        return total - allocated
    
    @staticmethod
    def get_vram_usage_percent() -> float:
        """Lấy phần trăm VRAM đã sử dụng"""
        total = VRAMManager.get_total_vram_mb()
        if total == 0:
            return 0
        allocated = VRAMManager.get_allocated_vram_mb()
        return (allocated / total) * 100
    
    @staticmethod
    def release():
        """
        Giải phóng VRAM triệt để
        
        Được gọi sau mỗi step để đảm bảo:
        - gc.collect() xóa Python objects
        - torch.cuda.empty_cache() xóa cache PyTorch
        - torch.cuda.synchronize() đợi tất cả operations hoàn thành
        """
        # Garbage collection
        gc.collect()
        
        if torch.cuda.is_available():
            # Xóa cache
            torch.cuda.empty_cache()
            
            # Đợi tất cả operations hoàn thành
            try:
                torch.cuda.synchronize()
            except:
                pass
        
        if VRAMManager._debug:
            free = VRAMManager.get_free_vram_mb()
            print(f"    ♻️  VRAM released (free: {free:.0f}MB)")
    
    @staticmethod
    def can_allocate(
        model_name: str, 
        safety_margin: Optional[float] = None
    ) -> bool:
        """
        Kiểm tra có đủ VRAM cho model không
        
        Args:
            model_name: Tên model (key trong VRAM_REQUIREMENTS)
            safety_margin: Hệ số an toàn (default: 1.3)
            
        Returns:
            True nếu có đủ VRAM
        """
        if safety_margin is None:
            safety_margin = VRAMManager.DEFAULT_SAFETY_MARGIN
        
        required = VRAMManager.VRAM_REQUIREMENTS.get(model_name, 100)
        return VRAMManager.get_free_vram_mb() >= required * safety_margin
    
    @staticmethod
    def wait_for_vram(
        required_mb: float, 
        timeout_sec: Optional[int] = None
    ) -> bool:
        """
        Chờ cho đến khi có đủ VRAM
        
        Args:
            required_mb: VRAM cần thiết (MB)
            timeout_sec: Thời gian tối đa chờ (default: 300s)
            
        Returns:
            True nếu đủ VRAM
            False nếu timeout
            
        Raises:
            TimeoutError: Nếu không đủ VRAM sau timeout
        """
        if timeout_sec is None:
            timeout_sec = VRAMManager.TIMEOUT_SEC
        
        start_time = time.time()
        check_count = 0
        
        while True:
            free = VRAMManager.get_free_vram_mb()
            
            if free >= required_mb:
                return True
            
            # Kiểm tra timeout
            elapsed = time.time() - start_time
            if elapsed > timeout_sec:
                raise TimeoutError(
                    f"Không đủ VRAM sau {timeout_sec}s. "
                    f"Cần: {required_mb:.0f}MB, Có: {free:.0f}MB"
                )
            
            # Log progress mỗi 10 lần
            check_count += 1
            if check_count % 10 == 0:
                print(f"    ⏳ Chờ VRAM... ({free:.0f}MB / {required_mb:.0f}MB, "
                      f"đã chờ: {elapsed:.0f}s)")
            
            # Sleep ngắn để không spam
            time.sleep(0.5)
    
    @staticmethod
    def get_required_vram(model_name: str) -> float:
        """Lấy VRAM yêu cầu của model (MB)"""
        return VRAMManager.VRAM_REQUIREMENTS.get(model_name, 100)
    
    @staticmethod
    def print_status():
        """In trạng thái VRAM hiện tại"""
        if not torch.cuda.is_available():
            print("    💾 GPU: Not available (using CPU)")
            return
        
        total = VRAMManager.get_total_vram_mb()
        allocated = VRAMManager.get_allocated_vram_mb()
        free = VRAMManager.get_free_vram_mb()
        usage = VRAMManager.get_vram_usage_percent()
        
        # Visual bar
        bar_length = 30
        filled = int(bar_length * usage / 100)
        bar = '█' * filled + '░' * (bar_length - filled)
        
        print(f"    💾 GPU Memory: [{bar}] {usage:.1f}%")
        print(f"    💾   Total: {total:.0f}MB | Allocated: {allocated:.0f}MB | Free: {free:.0f}MB")
    
    @staticmethod
    def get_model_recommendation(fallback_to_smaller: bool = True) -> str:
        """
        Gợi ý model phù hợp với VRAM hiện tại
        
        Args:
            fallback_to_smaller: Nếu True, gợi model nhỏ hơn
            
        Returns:
            Tên model được gợi ý
        """
        free = VRAMManager.get_free_vram_mb()
        
        # Tìm model phù hợp
        candidates = []
        for name, required in sorted(VRAMManager.VRAM_REQUIREMENTS.items(), 
                                       key=lambda x: x[1]):
            if free >= required * VRAMManager.DEFAULT_SAFETY_MARGIN:
                candidates.append((name, required))
        
        if candidates:
            # Trả về model lớn nhất có thể
            best = candidates[-1]
            return f"{best[0]} (~{best[1]}MB required)"
        
        if fallback_to_smaller:
            # Gợi model nhỏ nhất
            smallest = min(VRAMManager.VRAM_REQUIREMENTS.items(), key=lambda x: x[1])
            return f"{smallest[0]} (~{smallest[1]}MB required) - VRAM thấp"
        
        return "Không có model nào phù hợp"


# Context manager cho GPU lock
class GPULockContext:
    """Context manager cho GPU lock"""
    
    def __init__(self, vram_manager: VRAMManager):
        self.vram_manager = vram_manager
        self.allocated = False
    
    async def __aenter__(self):
        if VRAMManager.gpu_lock.locked():
            print("    ⏳ GPU đang bậy, đang chờ...")
        
        await VRAMManager.gpu_lock.acquire()
        self.allocated = True
        VRAMManager.print_status()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.allocated:
            VRAMManager.release()
            VRAMManager.gpu_lock.release()
        return False


# CLI for testing
if __name__ == "__main__":
    import asyncio
    
    async def test():
        print("VRAM Manager Test")
        print("=" * 50)
        
        manager = get_vram_manager()
        manager.print_status()
        
        print("\nModel Recommendations:")
        print(f"  → {manager.get_model_recommendation()}")
        
        print("\nChecking if we can allocate models...")
        
        for model in VRAMManager.VRAM_REQUIREMENTS.keys():
            can = VRAMManager.can_allocate(model)
            status = "✅" if can else "❌"
            required = VRAMManager.get_required_vram(model)
            print(f"  {status} {model}: {required}MB required")
        
        print("\nReleasing VRAM...")
        VRAMManager.release()
        
        print("\nGPU Lock Test:")
        
        async def demo_task(task_id: int):
            async with GPULockContext(manager):
                print(f"  🚀 Task {task_id} started")
                await asyncio.sleep(0.5)
                print(f"  ✅ Task {task_id} completed")
        
        # Run 3 tasks concurrently - they should run sequentially
        await asyncio.gather(
            demo_task(1),
            demo_task(2),
            demo_task(3),
        )
    
    asyncio.run(test())
