"""
GPT-SoVITS Core Runner - Headless Voice Cloning

Features:
- Chạy GPT-SoVITS không qua WebUI
- Sequential subprocess cho 4 bước training
- Hot-swap model mà không restart server
- VRAM-aware batch size adjustment

Lưu ý:
- GPT-SoVITS không có API chuẩn
- Phải chạy subprocess tuần tự 4 scripts
- Cần cài đặt GPT-SoVITS trước
"""

import asyncio
import subprocess
import shutil
import json
from pathlib import Path
from typing import Optional, List, Dict, Tuple
from dataclasses import dataclass
import torch

from .vram_manager import VRAMManager


@dataclass
class TrainingConfig:
    """Configuration cho GPT-SoVITS training"""
    # Paths
    gpt_sovits_dir: Path  # Thư mục GPT-SoVITS
    data_dir: Path  # Thư mục chứa data (raw/, metadata.csv)
    output_dir: Path  # Thư mục output model
    
    # Model params
    bert_size: str = "chinese-roberta-wwm-ext-large"
    num_layers: int = 6
    dialogue_layer: int = 6
    train_steps: int = 1000
    save_steps: int = 100
    batch_size: int = 4
    
    # Vietnamese optimizations
    use_vietnamese_phoneme: bool = True
    target_sr: int = 16000
    
    # Inference
    prompt_text: str = ""  # Prompt text for inference
    prompt_audio: str = ""  # Prompt audio path


@dataclass
class TrainingProgress:
    """Progress của training"""
    step: int
    total_steps: int
    loss: float
    percent: float


class GPTSoVITSCore:
    """
    GPT-SoVITS Core - Headless Voice Cloning
    
    4-Step Training Pipeline:
    1. Semantic Token Extraction (HuBERT)
    2. Acoustic Feature Extraction (VITS)
    3. SoVITS Training
    4. GPT Training
    
    Usage:
        core = GPTSoVITSCore(gpt_sovits_dir="path/to/GPT-SoVITS")
        
        config = TrainingConfig(
            gpt_sovits_dir=Path("..."),
            data_dir=Path("..."),
            output_dir=Path("...")
        )
        
        await core.train(config)
    """
    
    # Step names
    STEPS = [
        "Semantic Token Extraction",
        "Acoustic Feature Extraction", 
        "SoVITS Training",
        "GPT Training"
    ]
    
    def __init__(self, gpt_sovits_dir: Optional[Path] = None):
        """
        Initialize GPT-SoVITS Core
        
        Args:
            gpt_sovits_dir: Đường dẫn đến thư mục GPT-SoVITS
        """
        self.gpt_sovits_dir = gpt_sovits_dir or self._find_gpt_sovits()
        self.initialized = self.gpt_sovits_dir is not None
        
        if not self.initialized:
            print("    ⚠️  GPT-SoVITS not found")
            print("    📝 Download from: https://github.com/Soulghost/GPT-SoVITS")
    
    def _find_gpt_sovits(self) -> Optional[Path]:
        """Tìm thư mục GPT-SoVITS"""
        candidates = [
            Path("./GPT-SoVITS"),
            Path("./gpt_sovits"),
            Path("../GPT-SoVITS"),
            Path.home() / "GPT-SoVITS",
        ]
        
        for candidate in candidates:
            if candidate.exists():
                return candidate
        
        return None
    
    # ═══════════════════════════════════════════════════════════════════════════
    # DATA PREPARATION
    # ═══════════════════════════════════════════════════════════════════════════
    
    def prepare_data(
        self,
        audio_chunks: List[Path],
        texts: List[str],
        output_dir: Path
    ) -> TrainingConfig:
        """
        Chuẩn bị data cho GPT-SoVITS
        
        Args:
            audio_chunks: Danh sách đường dẫn audio chunks
            texts: Danh sách text tương ứng
            output_dir: Thư mục output
            
        Returns:
            TrainingConfig với paths đã setup
        """
        output_dir.mkdir(parents=True, exist_ok=True)
        raw_dir = output_dir / "raw"
        raw_dir.mkdir(exist_ok=True)
        
        # Copy audio files
        print(f"    📦 Copying {len(audio_chunks)} audio files...")
        for i, audio_path in enumerate(audio_chunks):
            dest = raw_dir / f"{i:04d}.wav"
            shutil.copy(audio_path, dest)
        
        # Create metadata.csv
        metadata_path = output_dir / "metadata.csv"
        with open(metadata_path, 'w', encoding='utf-8') as f:
            for i, text in enumerate(texts):
                # Format: filename|text
                f.write(f"{i:04d}.wav|{text}\n")
        
        print(f"    ✅ Data prepared: {len(audio_chunks)} samples")
        print(f"    📁 Location: {output_dir}")
        
        return TrainingConfig(
            gpt_sovits_dir=self.gpt_sovits_dir,
            data_dir=output_dir,
            output_dir=output_dir / "models"
        )
    
    # ═══════════════════════════════════════════════════════════════════════════
    # TRAINING STEPS
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def run_subprocess(
        self,
        cmd: List[str],
        step_name: str,
        env: Optional[Dict] = None
    ) -> Tuple[int, str]:
        """
        Chạy subprocess với logging
        
        Args:
            cmd: Command để chạy
            step_name: Tên step (để log)
            env: Environment variables
            
        Returns:
            (return_code, output)
        """
        print(f"    🚀 Running: {step_name}")
        
        # Merge environment
        process_env = None
        if env:
            import os
            process_env = {**os.environ, **env}
        
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=process_env,
        )
        
        stdout, stderr = await process.communicate()
        
        output = stdout.decode() + stderr.decode()
        
        if process.returncode != 0:
            print(f"    ❌ Failed: {step_name}")
            print(f"    Error: {stderr.decode()[:500]}")
            return process.returncode, output
        
        return 0, output
    
    async def step1_extract_semantic(self, config: TrainingConfig) -> bool:
        """
        Step 1: Extract semantic tokens using HuBERT
        
        Args:
            config: Training configuration
            
        Returns:
            True nếu thành công
        """
        print(f"\n🧠 Step 1/4: Semantic Token Extraction (HuBERT)")
        print("-" * 40)
        
        # Check if GPT-SoVITS is available
        if not self.gpt_sovits_dir:
            print("    ⚠️  GPT-SoVITS not installed, skipping...")
            return False
        
        # Path to extract_semantic.py
        script = self.gpt_sovits_dir / "extract_semantic.py"
        
        if not script.exists():
            print(f"    ⚠️  Script not found: {script}")
            return False
        
        # Command
        cmd = [
            "python",
            str(script),
            "-i", str(config.data_dir),
            "-o", str(config.data_dir / "semantic"),
            "--bert_size", config.bert_size,
        ]
        
        returncode, output = await self.run_subprocess(
            cmd, 
            "Semantic Token Extraction"
        )
        
        return returncode == 0
    
    async def step2_extract_acoustic(self, config: TrainingConfig) -> bool:
        """
        Step 2: Extract acoustic features
        
        Args:
            config: Training configuration
            
        Returns:
            True nếu thành công
        """
        print(f"\n🎵 Step 2/4: Acoustic Feature Extraction (VITS)")
        print("-" * 40)
        
        if not self.gpt_sovits_dir:
            print("    ⚠️  GPT-SoVITS not installed, skipping...")
            return False
        
        # Path to extract_acoustic.py (trong GPT-SoVITS gốc gọi là get_mel.py hoặc tương tự)
        script = self.gpt_sovits_dir / "extract_feature.py"
        
        if not script.exists():
            print(f"    ⚠️  Script not found: {script}")
            return False
        
        cmd = [
            "python",
            str(script),
            "-i", str(config.data_dir / "raw"),
            "-o", str(config.data_dir / "mel"),
            "--sample_rate", str(config.target_sr),
        ]
        
        returncode, output = await self.run_subprocess(
            cmd,
            "Acoustic Feature Extraction"
        )
        
        return returncode == 0
    
    async def step3_train_sovits(self, config: TrainingConfig) -> bool:
        """
        Step 3: Train SoVITS model
        
        Args:
            config: Training configuration
            
        Returns:
            True nếu thành công
        """
        print(f"\n🔊 Step 3/4: SoVITS Training")
        print("-" * 40)
        
        if not self.gpt_sovits_dir:
            print("    ⚠️  GPT-SoVITS not installed, skipping...")
            return False
        
        script = self.gpt_sovits_dir / "train_sovits.py"
        
        if not script.exists():
            print(f"    ⚠️  Script not found: {script}")
            return False
        
        # Calculate optimal batch size based on VRAM
        free_vram = VRAMManager.get_free_vram_mb()
        if free_vram < 2000:
            config.batch_size = 2
            print(f"    ⚠️  Low VRAM ({free_vram:.0f}MB), reducing batch size to {config.batch_size}")
        else:
            config.batch_size = 4
        
        cmd = [
            "python",
            str(script),
            "-i", str(config.data_dir),
            "-o", str(config.output_dir / "sovits"),
            "--batch_size", str(config.batch_size),
            "--train_steps", str(config.train_steps),
            "--save_steps", str(config.save_steps),
        ]
        
        returncode, output = await self.run_subprocess(
            cmd,
            "SoVITS Training"
        )
        
        return returncode == 0
    
    async def step4_train_gpt(self, config: TrainingConfig) -> bool:
        """
        Step 4: Train GPT model
        
        Args:
            config: Training configuration
            
        Returns:
            True nếu thành công
        """
        print(f"\n🤖 Step 4/4: GPT Training")
        print("-" * 40)
        
        if not self.gpt_sovits_dir:
            print("    ⚠️  GPT-SoVITS not installed, skipping...")
            return False
        
        script = self.gpt_sovits_dir / "train_gpt.py"
        
        if not script.exists():
            print(f"    ⚠️  Script not found: {script}")
            return False
        
        cmd = [
            "python",
            str(script),
            "-i", str(config.data_dir),
            "-o", str(config.output_dir / "gpt"),
            "--bert_size", config.bert_size,
            "--num_layers", str(config.num_layers),
            "--dialogue_layer", str(config.dialogue_layer),
            "--batch_size", str(config.batch_size),
            "--train_steps", str(config.train_steps),
            "--save_steps", str(config.save_steps),
        ]
        
        returncode, output = await self.run_subprocess(
            cmd,
            "GPT Training"
        )
        
        return returncode == 0
    
    # ═══════════════════════════════════════════════════════════════════════════
    # FULL TRAINING PIPELINE
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def train(
        self,
        config: TrainingConfig,
        progress_callback: Optional[callable] = None
    ) -> Path:
        """
        Full training pipeline
        
        Args:
            config: Training configuration
            progress_callback: Callback để report progress
            
        Returns:
            Path đến trained model
        """
        print(f"\n{'='*50}")
        print(f"🧠 GPT-SoVITS TRAINING")
        print(f"{'='*50}")
        print(f"   Data: {config.data_dir}")
        print(f"   Output: {config.output_dir}")
        print(f"   Batch size: {config.batch_size}")
        print(f"   Train steps: {config.train_steps}")
        print(f"{'='*50}")
        
        # Check GPU
        if torch.cuda.is_available():
            VRAMManager.print_status()
        
        # Ensure output dir exists
        config.output_dir.mkdir(parents=True, exist_ok=True)
        
        # Step 1: Extract semantic tokens
        success1 = await self.step1_extract_semantic(config)
        if not success1:
            print("    ⚠️  Step 1 failed, continuing...")
        
        # Step 2: Extract acoustic features
        success2 = await self.step2_extract_acoustic(config)
        if not success2:
            print("    ⚠️  Step 2 failed, continuing...")
        
        # Step 3: Train SoVITS
        success3 = await self.step3_train_sovits(config)
        if not success3:
            print("    ⚠️  Step 3 failed, continuing...")
        
        # Step 4: Train GPT
        success4 = await self.step4_train_gpt(config)
        if not success4:
            print("    ⚠️  Step 4 failed, continuing...")
        
        # Check if training succeeded
        if success3 and success4:
            print(f"\n✅ Training completed!")
            
            # Return path to model
            return config.output_dir
        else:
            print(f"\n⚠️  Training completed with errors")
            return config.output_dir
    
    # ═══════════════════════════════════════════════════════════════════════════
    # INFERENCE
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def inference(
        self,
        model_dir: Path,
        text: str,
        reference_audio: Path,
        output_path: Path,
        prompt_text: str = "",
    ) -> Path:
        """
        Generate audio from text using trained model
        
        Args:
            model_dir: Thư mục chứa trained model
            text: Text cần synthesize
            reference_audio: Reference audio để clone voice
            output_path: Output path
            prompt_text: Prompt text (optional)
            
        Returns:
            Path đến generated audio
        """
        print(f"\n🎤 INFERENCE")
        print("-" * 40)
        
        if not self.gpt_sovits_dir:
            raise RuntimeError("GPT-SoVITS not installed")
        
        script = self.gpt_sovits_dir / "inference.py"
        
        if not script.exists():
            raise RuntimeError(f"Script not found: {script}")
        
        cmd = [
            "python",
            str(script),
            "-m", str(model_dir),
            "-t", text,
            "-r", str(reference_audio),
            "-o", str(output_path),
        ]
        
        if prompt_text:
            cmd.extend(["-p", prompt_text])
        
        returncode, output = await self.run_subprocess(cmd, "Inference")
        
        if returncode != 0:
            raise RuntimeError(f"Inference failed: {output}")
        
        return output_path
    
    # ═══════════════════════════════════════════════════════════════════════════
    # HOT-SWAP
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def hot_swap(
        self,
        model_path: Path,
        voice_name: str
    ) -> Dict[str, Path]:
        """
        Hot-swap model mà không cần restart server
        
        Args:
            model_path: Path đến model
            voice_name: Tên voice
            
        Returns:
            Dict với paths của các component models
        """
        print(f"\n🔄 HOT-SWAPPING MODEL: {voice_name}")
        print("-" * 40)
        
        # Copy model files to active directory
        active_dir = Path("./models/active")
        active_dir.mkdir(parents=True, exist_ok=True)
        
        # Find model files
        sovits_path = model_path / "sovits" / "model.pth"
        gpt_path = model_path / "gpt" / "model.pth"
        
        if sovits_path.exists():
            dest_sovits = active_dir / f"{voice_name}_sovits.pth"
            shutil.copy(sovits_path, dest_sovits)
            print(f"    ✅ Copied SoVITS: {dest_sovits}")
        
        if gpt_path.exists():
            dest_gpt = active_dir / f"{voice_name}_gpt.pth"
            shutil.copy(gpt_path, dest_gpt)
            print(f"    ✅ Copied GPT: {dest_gpt}")
        
        # Save config
        config_path = active_dir / f"{voice_name}_config.json"
        config = {
            "voice_name": voice_name,
            "sovits": str(dest_sovits) if sovits_path.exists() else None,
            "gpt": str(dest_gpt) if gpt_path.exists() else None,
            "model_dir": str(model_path),
        }
        
        with open(config_path, 'w') as f:
            json.dump(config, f, indent=2)
        
        print(f"    ✅ Hot-swap complete: {voice_name}")
        
        return {
            "sovits": dest_sovits if sovits_path.exists() else None,
            "gpt": dest_gpt if gpt_path.exists() else None,
            "config": config_path,
        }


# CLI for testing
if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="GPT-SoVITS Core Test")
    parser.add_argument("--gpt-dir", help="GPT-SoVITS directory")
    parser.add_argument("--data-dir", help="Data directory")
    parser.add_argument("--output-dir", help="Output directory")
    
    args = parser.parse_args()
    
    async def test():
        core = GPTSoVITSCore(
            gpt_sovits_dir=Path(args.gpt_dir) if args.gpt_dir else None
        )
        
        if core.initialized:
            print(f"✅ GPT-SoVITS found: {core.gpt_sovits_dir}")
        else:
            print("❌ GPT-SoVITS not found")
            print("📝 Download from: https://github.com/Soulghost/GPT-SoVITS")
    
    asyncio.run(test())
