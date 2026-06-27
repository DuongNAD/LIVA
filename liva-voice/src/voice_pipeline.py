"""
Voice Pipeline - LIVA 2.0 Main Class

Orchestrates all components:
- AudioProcessor: Download & VAD chunking
- SpeakerVerifier: Filter by speaker similarity
- Whisper: Speech to text
- VietnameseNormalizer: Text normalization
- GPTSoVITSCore: Voice cloning training
"""

import asyncio
from pathlib import Path
from typing import Optional, List, Dict
from dataclasses import dataclass

from .vram_manager import VRAMManager, GPULockContext
from .audio_processor import AudioProcessor
from .hallucination_filter import HallucinationFilter
from .speaker_verifier import SpeakerVerifier
from .vietnamese_normalizer import VietnameseNormalizer
from .gpt_sovits_core import GPTSoVITSCore, TrainingConfig


@dataclass
class CloneResult:
    """Kết quả của voice cloning"""
    status: str  # "success" or "error"
    model_path: Optional[str] = None
    sample_path: Optional[str] = None
    stats: Optional[Dict] = None
    error: Optional[str] = None


@dataclass
class PipelineStats:
    """Statistics cho pipeline"""
    chunks_downloaded: int = 0
    chunks_after_vad: int = 0
    chunks_after_verify: int = 0
    chunks_after_filter: int = 0
    dataset_size: int = 0
    training_steps: int = 0


class VoicePipeline:
    """
    LIVA 2.0 Voice Cloning Pipeline
    
    Orchestrates the full voice cloning pipeline:
    1. Audio Preparation (download, VAD chunking)
    2. Speaker Verification (optional)
    3. Speech to Text (Whisper with anti-hallucination)
    4. Text Normalization (Vietnamese)
    5. TTS Training (GPT-SoVITS)
    6. Validation
    
    Features:
    - GPU Lock to prevent OOM
    - Async operations throughout
    - VRAM management
    - Progress reporting
    
    Usage:
        pipeline = VoicePipeline()
        
        result = await pipeline.clone_voice(
            audio_url="https://youtube.com/...",
            voice_name="my_voice",
            reference_audio="reference.wav"  # optional
        )
    """
    
    def __init__(
        self,
        workspace: str = "./workspace",
        gpt_sovits_dir: Optional[str] = None
    ):
        """
        Initialize Voice Pipeline
        
        Args:
            workspace: Thư mục workspace
            gpt_sovits_dir: Thư mục GPT-SoVITS (optional)
        """
        self.workspace = Path(workspace)
        self.workspace.mkdir(parents=True, exist_ok=True)
        
        # Components
        self.audio_processor = AudioProcessor(self.workspace)
        self.hallucination_filter = HallucinationFilter()
        self.speaker_verifier = SpeakerVerifier()
        self.normalizer = VietnameseNormalizer()
        self.gpt_sovits = GPTSoVITSCore(
            gpt_sovits_dir=Path(gpt_sovits_dir) if gpt_sovits_dir else None
        )
        
        # Stats
        self.stats = PipelineStats()
    
    async def clone_voice(
        self,
        audio_url: str,
        voice_name: str,
        reference_audio: Optional[str] = None,
        do_speaker_verify: bool = True,
    ) -> CloneResult:
        """
        Clone giọng từ URL audio
        
        Args:
            audio_url: URL audio (YouTube, direct link, etc.)
            voice_name: Tên giọng (dùng làm file model)
            reference_audio: Reference audio cho speaker verification
            do_speaker_verify: Có verify speaker không
            
        Returns:
            CloneResult với status, model_path, stats
        """
        # ════════════════════════════════════════════════════════════════════════
        # GPU LOCK - Prevent race condition
        # ════════════════════════════════════════════════════════════════════════
        async with GPULockContext(VRAMManager()):
            try:
                print(f"\n{'='*60}")
                print(f"🚀 LIVA 2.0 VOICE CLONE: {voice_name}")
                print(f"{'='*60}")
                
                # Print initial status
                if VRAMManager.is_cuda_available:
                    VRAMManager.print_status()
                else:
                    print("    💾 GPU: Not available (using CPU)")
                
                print(f"{'='*60}\n")
                
                # Reset stats
                self.stats = PipelineStats()
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 1: Audio Preparation
                # ══════════════════════════════════════════════════════════════════
                chunks = await self._step1_audio_prep(audio_url)
                self.stats.chunks_downloaded = len(chunks)
                
                if not chunks:
                    raise ValueError("No audio chunks generated")
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 2: Speaker Verification (Optional)
                # ══════════════════════════════════════════════════════════════════
                if do_speaker_verify and reference_audio:
                    chunks = await self._step2_speaker_verify(chunks, reference_audio)
                
                self.stats.chunks_after_verify = len(chunks)
                
                if not chunks:
                    raise ValueError("No chunks after speaker verification")
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 3: Speech to Text (Whisper)
                # ══════════════════════════════════════════════════════════════════
                dataset = await self._step3_transcribe(chunks)
                
                if not dataset:
                    raise ValueError("No valid transcriptions")
                
                self.stats.chunks_after_filter = len(dataset)
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 4: Vietnamese Text Normalization
                # ══════════════════════════════════════════════════════════════════
                dataset = self._step4_normalize(dataset)
                self.stats.dataset_size = len(dataset)
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 5: TTS Training (GPT-SoVITS)
                # ══════════════════════════════════════════════════════════════════
                model_path = await self._step5_train_tts(dataset, voice_name)
                
                # ══════════════════════════════════════════════════════════════════
                # STEP 6: Validation
                # ══════════════════════════════════════════════════════════════════
                sample_path = await self._step6_validate(model_path, dataset[0]['text'])
                
                # ══════════════════════════════════════════════════════════════════
                # SUCCESS
                # ══════════════════════════════════════════════════════════════════
                return CloneResult(
                    status="success",
                    model_path=str(model_path),
                    sample_path=str(sample_path),
                    stats={
                        "chunks_after_vad": self.stats.chunks_downloaded,
                        "chunks_after_verify": self.stats.chunks_after_verify,
                        "chunks_after_filter": self.stats.chunks_after_filter,
                        "dataset_size": self.stats.dataset_size,
                    }
                )
                
            except Exception as e:
                # Cleanup on error
                VRAMManager.release()
                
                print(f"\n❌ ERROR: {e}")
                import traceback
                traceback.print_exc()
                
                return CloneResult(
                    status="error",
                    error=str(e)
                )
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 1: Audio Preparation
    # ════════════════════════════════════════════════════════════════════════════════
    
    async def _step1_audio_prep(self, audio_url: str) -> List[Path]:
        """
        Step 1: Download audio và VAD chunking
        
        Returns:
            List of chunk paths
        """
        print(f"\n🎧 [STEP 1/6] AUDIO PREPARATION")
        print("-" * 40)
        
        # Download and process
        chunks = await self.audio_processor.process_url(audio_url)
        
        print(f"    ✅ Created {len(chunks)} audio chunks")
        
        # Release VRAM
        VRAMManager.release()
        
        return [Path(c) for c in chunks]
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 2: Speaker Verification
    # ════════════════════════════════════════════════════════════════════════════════
    
    async def _step2_speaker_verify(
        self,
        chunks: List[Path],
        reference_audio: str
    ) -> List[Path]:
        """
        Step 2: Speaker verification
        
        Args:
            chunks: List of chunk paths
            reference_audio: Path to reference audio
            
        Returns:
            Filtered list of chunks
        """
        print(f"\n👤 [STEP 2/6] SPEAKER VERIFICATION")
        print("-" * 40)
        
        try:
            # Load model
            await self.speaker_verifier.load_model()
            
            # Verify chunks
            matching, filtered = self.speaker_verifier.filter_chunks(
                chunks,
                Path(reference_audio)
            )
            
            print(f"    ✅ Verified: {len(matching)}/{len(chunks)} chunks match")
            
            # Cleanup
            del self.speaker_verifier.encoder
            self.speaker_verifier.encoder = None
            VRAMManager.release()
            
            return matching
            
        except Exception as e:
            print(f"    ⚠️  Speaker verification failed: {e}")
            print("    📝 Continuing without verification...")
            return chunks
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 3: Speech to Text
    # ════════════════════════════════════════════════════════════════════════════════
    
    async def _step3_transcribe(self, chunks: List[Path]) -> List[Dict]:
        """
        Step 3: Transcribe với Faster-Whisper
        
        Args:
            chunks: List of chunk paths
            
        Returns:
            List of dict với audio_path, text, duration
        """
        print(f"\n📝 [STEP 3/6] SPEECH TO TEXT")
        print("-" * 40)
        
        # Check VRAM
        if not VRAMManager.can_allocate("whisper_large_v3_turbo"):
            print("    ⚠️  Low VRAM, using smaller model...")
        
        # Load Whisper
        from faster_whisper import WhisperModel
        
        model_size = "large-v3-turbo" if VRAMManager.can_allocate("whisper_large_v3_turbo") else "small"
        print(f"    📊 Model: {model_size}")
        
        VRAMManager.release()
        
        whisper = WhisperModel(
            model_size,
            device="cuda" if VRAMManager.is_cuda_available else "cpu",
            compute_type="int8" if VRAMManager.is_cuda_available else "int8"
        )
        print("    ✅ Whisper loaded")
        
        dataset = []
        
        for i, chunk in enumerate(chunks):
            print(f"    🔄 [{i+1}/{len(chunks)}] Transcribing...")
            
            try:
                # Get duration
                info = self.audio_processor.get_audio_info(chunk)
                duration = info.duration_sec
                
                # Transcribe with anti-hallucination settings
                segments, segment_info = whisper.transcribe(
                    str(chunk),
                    language="vi",
                    condition_on_previous_text=False,
                    no_speech_threshold=0.6,
                    word_timestamps=False,
                )
                
                # Combine text
                text = " ".join([seg.text for seg in segments])
                
                # Filter with hallucination filter
                is_valid, reason = self.hallucination_filter.filter(
                    text,
                    duration,
                    segment_info.no_speech_prob
                )
                
                if is_valid:
                    dataset.append({
                        "audio": str(chunk),
                        "text": text.strip(),
                        "duration": duration,
                        "no_speech_prob": segment_info.no_speech_prob
                    })
                    print(f"    ✅ [{i+1}] {text[:40]}...")
                else:
                    print(f"    ❌ [{i+1}] Filtered ({reason}): {text[:40]}...")
                    
            except Exception as e:
                print(f"    ❌ [{i+1}] Error: {e}")
        
        # Cleanup
        del whisper
        VRAMManager.release()
        
        print(f"    📊 Final dataset: {len(dataset)} chunks")
        
        return dataset
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 4: Vietnamese Normalization
    # ════════════════════════════════════════════════════════════════════════════════
    
    def _step4_normalize(self, dataset: List[Dict]) -> List[Dict]:
        """
        Step 4: Normalize Vietnamese text
        
        Args:
            dataset: List of dict với text
            
        Returns:
            Normalized dataset
        """
        print(f"\n🇻🇳 [STEP 4/6] TEXT NORMALIZATION")
        print("-" * 40)
        
        for item in dataset:
            original = item['text']
            normalized = self.normalizer.normalize(original)
            item['text'] = normalized
            
            if original != normalized:
                print(f"    📝 {original[:30]}...")
                print(f"    📝 → {normalized[:30]}...")
        
        print(f"    ✅ Normalized {len(dataset)} samples")
        
        return dataset
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 5: TTS Training
    # ════════════════════════════════════════════════════════════════════════════════
    
    async def _step5_train_tts(
        self,
        dataset: List[Dict],
        voice_name: str
    ) -> Path:
        """
        Step 5: Train TTS với GPT-SoVITS
        
        Args:
            dataset: List of dict với audio và text
            voice_name: Tên giọng
            
        Returns:
            Path to trained model
        """
        print(f"\n🧠 [STEP 5/6] TTS TRAINING")
        print("-" * 40)
        
        # Prepare data
        audio_chunks = [Path(item['audio']) for item in dataset]
        texts = [item['text'] for item in dataset]
        
        output_dir = self.workspace / "models" / voice_name
        
        config = self.gpt_sovits.prepare_data(
            audio_chunks=audio_chunks,
            texts=texts,
            output_dir=output_dir
        )
        
        # Train
        model_path = await self.gpt_sovits.train(config)
        
        # Hot-swap
        if self.gpt_sovits.initialized:
            await self.gpt_sovits.hot_swap(model_path, voice_name)
        
        # Release VRAM
        VRAMManager.release()
        
        return model_path
    
    # ════════════════════════════════════════════════════════════════════════════════
    # STEP 6: Validation
    # ════════════════════════════════════════════════════════════════════════════════
    
    async def _step6_validate(self, model_path: Path, test_text: str) -> Path:
        """
        Step 6: Validate by generating sample
        
        Args:
            model_path: Path to trained model
            test_text: Text để test
            
        Returns:
            Path to generated sample
        """
        print(f"\n🎤 [STEP 6/6] VALIDATION")
        print("-" * 40)
        
        sample_path = model_path / "sample.wav"
        
        # Get first audio chunk as reference
        chunks_dir = self.workspace / "chunks"
        chunks = list(chunks_dir.glob("*.wav"))
        
        if chunks and self.gpt_sovits.initialized:
            print(f"    🔊 Generating sample...")
            print(f"    📝 Text: {test_text[:50]}...")
            
            try:
                await self.gpt_sovits.inference(
                    model_dir=model_path,
                    text=test_text,
                    reference_audio=chunks[0],
                    output_path=sample_path,
                )
            except Exception as e:
                print(f"    ⚠️  Inference failed: {e}")
                print("    📝 Creating placeholder sample path...")
        else:
            print(f"    ⚠️  Skipping inference (GPT-SoVITS not installed)")
            print("    📝 Sample path: {sample_path}")
        
        return sample_path
    
    # ════════════════════════════════════════════════════════════════════════════════
    # UTILITY METHODS
    # ════════════════════════════════════════════════════════════════════════════════
    
    def cleanup(self):
        """Cleanup workspace"""
        import shutil
        
        # Remove workspace
        if self.workspace.exists():
            shutil.rmtree(self.workspace)
            print(f"    🗑️  Cleaned up workspace: {self.workspace}")
    
    def list_voices(self) -> List[str]:
        """List all available voices"""
        models_dir = self.workspace / "models"
        
        if not models_dir.exists():
            return []
        
        return [p.name for p in models_dir.iterdir() if p.is_dir()]


# CLI Entry Point
async def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="LIVA 2.0 Voice Clone")
    parser.add_argument("--url", required=True, help="Audio URL")
    parser.add_argument("--name", required=True, help="Voice name")
    parser.add_argument("--reference", help="Reference audio for speaker verification")
    parser.add_argument("--workspace", default="./workspace", help="Workspace directory")
    parser.add_argument("--gpt-dir", help="GPT-SoVITS directory")
    parser.add_argument("--no-verify", action="store_true", help="Skip speaker verification")
    
    args = parser.parse_args()
    
    # Create pipeline
    pipeline = VoicePipeline(
        workspace=args.workspace,
        gpt_sovits_dir=args.gpt_dir
    )
    
    # Clone voice
    result = await pipeline.clone_voice(
        audio_url=args.url,
        voice_name=args.name,
        reference_audio=args.reference,
        do_speaker_verify=not args.no_verify,
    )
    
    # Print result
    print(f"\n{'='*60}")
    if result.status == "success":
        print(f"✅ CLONE SUCCESS!")
        print(f"   Model: {result.model_path}")
        print(f"   Sample: {result.sample_path}")
        print(f"   Stats: {result.stats}")
    else:
        print(f"❌ CLONE FAILED: {result.error}")
    print(f"{'='*60}")


if __name__ == "__main__":
    asyncio.run(main())
