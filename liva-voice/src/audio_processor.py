"""
Audio Processor - Xử lý âm thanh

Features:
- Download audio với async subprocess
- Resample về 16kHz Mono (chuẩn cho Whisper/Silero)
- Padding 200ms để tránh cắt lẹm
- Chunking với Silero VAD
"""

import asyncio
import subprocess
import torch
import torchaudio
from pathlib import Path
from typing import Optional, List, Tuple
from dataclasses import dataclass

# Constants
TARGET_SAMPLE_RATE = 16000
TARGET_CHANNELS = 1
PADDING_MS = 200  # 200ms padding cho mỗi chunk


@dataclass
class AudioInfo:
    """Thông tin về file audio"""
    path: Path
    duration_sec: float
    sample_rate: int
    channels: int
    num_frames: int


class AudioProcessor:
    """
    Audio Processing Utilities
    
    Features:
    - Async download với yt-dlp
    - Direct resample về 16kHz Mono
    - Add padding để tránh cắt lẹm
    - VAD chunking với Silero VAD
    """
    
    # Target format
    TARGET_SAMPLE_RATE = TARGET_SAMPLE_RATE
    TARGET_CHANNELS = TARGET_CHANNELS
    PADDING_MS = PADDING_MS
    
    def __init__(self, workspace: Optional[Path] = None):
        self.workspace = workspace or Path("./workspace")
        self.workspace.mkdir(parents=True, exist_ok=True)
    
    # ═══════════════════════════════════════════════════════════════════════════
    # AUDIO I/O
    # ═══════════════════════════════════════════════════════════════════════════
    
    @staticmethod
    def get_audio_info(audio_path: Path) -> AudioInfo:
        """Lấy thông tin về file audio"""
        info = torchaudio.info(str(audio_path))
        duration = info.num_frames / info.sample_rate
        
        return AudioInfo(
            path=audio_path,
            duration_sec=duration,
            sample_rate=info.sample_rate,
            channels=info.num_channels,
            num_frames=info.num_frames,
        )
    
    @staticmethod
    def load_audio(audio_path: Path) -> Tuple[torch.Tensor, int]:
        """Load audio file"""
        waveform, sr = torchaudio.load(str(audio_path))
        return waveform, sr
    
    @staticmethod
    def save_audio(
        audio_path: Path, 
        waveform: torch.Tensor, 
        sample_rate: int
    ):
        """Save audio file"""
        audio_path.parent.mkdir(parents=True, exist_ok=True)
        torchaudio.save(str(audio_path), waveform, sample_rate)
    
    @staticmethod
    def resample(
        waveform: torch.Tensor, 
        orig_sr: int, 
        target_sr: int = TARGET_SAMPLE_RATE
    ) -> Tuple[torch.Tensor, int]:
        """
        Resample audio về target sample rate
        
        Args:
            waveform: Audio tensor (channels, samples)
            orig_sr: Sample rate gốc
            target_sr: Sample rate đích (default: 16000)
            
        Returns:
            Resampled waveform and new sample rate
        """
        if orig_sr == target_sr:
            return waveform, target_sr
        
        # Use torchaudio resampler
        resampler = torchaudio.transforms.Resample(
            orig_freq=orig_sr,
            new_freq=target_sr
        )
        
        waveform_resampled = resampler(waveform)
        return waveform_resampled, target_sr
    
    @staticmethod
    def to_mono(
        waveform: torch.Tensor, 
        target_channels: int = TARGET_CHANNELS
    ) -> torch.Tensor:
        """Chuyển audio về mono"""
        if waveform.shape[0] == target_channels:
            return waveform
        
        if target_channels == 1:
            # Average channels
            return waveform.mean(dim=0, keepdim=True)
        
        return waveform
    
    # ═══════════════════════════════════════════════════════════════════════════
    # DOWNLOAD
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def download(
        self, 
        url: str, 
        output_path: Optional[Path] = None
    ) -> Path:
        """
        Download audio từ URL với async subprocess
        
        Args:
            url: URL audio/video (YouTube, direct link, etc.)
            output_path: Đường dẫn output (optional)
            
        Returns:
            Path đến file audio đã download
            
        Raises:
            RuntimeError: Nếu yt-dlp fail
        """
        if output_path is None:
            output_path = self.workspace / "raw" / "download.wav"
        
        output_path.parent.mkdir(parents=True, exist_ok=True)
        
        # Chuẩn bị command
        cmd = [
            "yt-dlp",
            "-x",                          # Extract audio
            "--audio-format", "wav",       # Output format
            "--postprocessor-args",         # FFmpeg args
            f"-ar {TARGET_SAMPLE_RATE} -ac {TARGET_CHANNELS}",  # 16kHz Mono
            "-o", str(output_path),
            url
        ]
        
        print(f"    📥 Downloading: {url[:50]}...")
        
        # Run async
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )
        
        stdout, stderr = await process.communicate()
        
        if process.returncode != 0:
            error = stderr.decode() if stderr else "Unknown error"
            
            # Check if yt-dlp not found
            if "yt-dlp" in error.lower() or process.returncode == 127:
                raise RuntimeError(
                    "yt-dlp not found. Please install: pip install yt-dlp"
                )
            
            raise RuntimeError(f"yt-dlp failed: {error}")
        
        # Verify output exists
        if not output_path.exists():
            raise RuntimeError(f"Download failed: {output_path} not created")
        
        print(f"    ✅ Downloaded: {output_path.name}")
        return output_path
    
    # ═══════════════════════════════════════════════════════════════════════════
    # PROCESSING
    # ═══════════════════════════════════════════════════════════════════════════
    
    def add_padding(
        self, 
        audio_path: Path, 
        padding_ms: int = PADDING_MS,
        output_path: Optional[Path] = None
    ) -> Path:
        """
        Thêm padding vào 2 đầu audio để tránh cắt lẹm
        
        Args:
            audio_path: File audio gốc
            padding_ms: Padding milliseconds (default: 200ms)
            output_path: File output (optional)
            
        Returns:
            Path đến file đã padding
        """
        if output_path is None:
            output_path = audio_path.parent / f"{audio_path.stem}_padded.wav"
        
        # Load audio
        waveform, sr = self.load_audio(audio_path)
        
        # Calculate padding samples
        padding_samples = int(sr * padding_ms / 1000)
        
        # Create padding tensors
        pad_front = torch.zeros(waveform.shape[0], padding_samples)
        pad_back = torch.zeros(waveform.shape[0], padding_samples)
        
        # Concatenate
        waveform_padded = torch.cat([pad_front, waveform, pad_back], dim=1)
        
        # Save
        self.save_audio(output_path, waveform_padded, sr)
        
        print(f"    ✅ Added {padding_ms}ms padding: {output_path.name}")
        return output_path
    
    def normalize_loudness(
        self,
        audio_path: Path,
        target_db: float = -20.0,
        output_path: Optional[Path] = None
    ) -> Path:
        """
        Normalize loudness của audio
        
        Args:
            audio_path: File audio gốc
            target_db: Target loudness in dB (default: -20 LUFS)
            output_path: File output
            
        Returns:
            Path đến file đã normalize
        """
        if output_path is None:
            output_path = audio_path.parent / f"{audio_path.stem}_norm.wav"
        
        # Load audio
        waveform, sr = self.load_audio(audio_path)
        
        # Calculate RMS
        rms = torch.sqrt(torch.mean(waveform ** 2))
        
        if rms > 0:
            # Calculate scale factor
            target_rms = 10 ** (target_db / 20)
            scale = target_rms / rms
            
            # Apply gain
            waveform_normalized = waveform * scale
            
            # Clip to prevent distortion
            waveform_normalized = torch.clamp(waveform_normalized, -1.0, 1.0)
        else:
            waveform_normalized = waveform
        
        # Save
        self.save_audio(output_path, waveform_normalized, sr)
        
        print(f"    ✅ Normalized loudness: {output_path.name}")
        return output_path
    
    # ═══════════════════════════════════════════════════════════════════════════
    # VAD CHUNKING
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def vad_chunking(
        self,
        audio_path: Path,
        output_dir: Optional[Path] = None,
        min_speech_duration_ms: int = 1000,
        max_speech_duration_s: float = 12.0,
        min_silence_duration_ms: int = 300,
        threshold: float = 0.5,
        padding_ms: int = PADDING_MS,
    ) -> List[Path]:
        """
        Cắt audio thành các chunk với Silero VAD
        
        Args:
            audio_path: File audio gốc
            output_dir: Thư mục chứa chunks
            min_speech_duration_ms: Minimum speech duration (default: 1000ms)
            max_speech_duration_s: Maximum speech duration (default: 12s)
            min_silence_duration_ms: Min silence between chunks (default: 300ms)
            threshold: VAD threshold (default: 0.5)
            padding_ms: Padding cho mỗi chunk (default: 200ms)
            
        Returns:
            List of chunk file paths
        """
        if output_dir is None:
            output_dir = self.workspace / "chunks"
        output_dir.mkdir(parents=True, exist_ok=True)
        
        try:
            from silero_vad import (
                load_silero_vad,
                get_speech_timestamps,
                read_audio,
                collect_chunks,
            )
            
            print("    📦 Loading Silero VAD...")
            vad_model = load_silero_vad()
            print("    ✅ Silero VAD loaded")
            
            # Read audio
            print(f"    📖 Reading audio: {audio_path.name}")
            audio = read_audio(str(audio_path), sampling_rate=TARGET_SAMPLE_RATE)
            audio_duration = len(audio) / TARGET_SAMPLE_RATE
            print(f"    📊 Duration: {audio_duration:.1f}s")
            
            # Get speech timestamps
            print(f"    🔍 Detecting speech (threshold={threshold})...")
            timestamps = get_speech_timestamps(
                audio,
                vad_model,
                threshold=threshold,
                min_speech_duration_ms=min_speech_duration_ms,
                max_speech_duration_s=max_speech_duration_s,
                min_silence_duration_ms=min_silence_duration_ms,
            )
            
            print(f"    📊 Found {len(timestamps)} speech segments")
            
            if not timestamps:
                print("    ⚠️  No speech detected!")
                return []
            
            # Export chunks with padding
            chunks = []
            padding_samples = int(TARGET_SAMPLE_RATE * padding_ms / 1000)
            
            for i, ts in enumerate(timestamps):
                # Add padding
                start = max(0, ts.start - padding_samples)
                end = min(len(audio), ts.end + padding_samples)
                
                # Extract chunk
                chunk_waveform = audio[start:end]
                
                # Save
                chunk_path = output_dir / f"chunk_{i:04d}.wav"
                torchaudio.save(
                    str(chunk_path),
                    chunk_waveform.unsqueeze(0),
                    TARGET_SAMPLE_RATE
                )
                chunks.append(chunk_path)
            
            print(f"    ✅ Created {len(chunks)} chunks (with {padding_ms}ms padding)")
            
            # Cleanup
            del vad_model
            
            return chunks
            
        except ImportError as e:
            print(f"    ⚠️  Silero VAD not available: {e}")
            print("    📝 Using manual chunking fallback...")
            return await self._manual_chunking(audio_path, output_dir)
    
    async def _manual_chunking(
        self,
        audio_path: Path,
        output_dir: Path,
        chunk_duration_sec: float = 10.0,
    ) -> List[Path]:
        """
        Fallback chunking không có Silero VAD
        
        Cắt audio thành các chunk 10 giây
        """
        output_dir.mkdir(parents=True, exist_ok=True)
        
        # Load audio
        waveform, sr = self.load_audio(audio_path)
        info = self.get_audio_info(audio_path)
        
        # Calculate chunk size
        chunk_samples = int(sr * chunk_duration_sec)
        total_samples = waveform.shape[1]
        
        chunks = []
        chunk_idx = 0
        
        for start in range(0, total_samples, chunk_samples):
            end = min(start + chunk_samples, total_samples)
            
            # Extract chunk
            chunk_waveform = waveform[:, start:end]
            
            # Skip if too short
            if chunk_waveform.shape[1] < sr:  # Less than 1 second
                continue
            
            # Save
            chunk_path = output_dir / f"chunk_{chunk_idx:04d}.wav"
            self.save_audio(chunk_path, chunk_waveform, sr)
            chunks.append(chunk_path)
            chunk_idx += 1
        
        print(f"    ✅ Manual chunking: {len(chunks)} chunks")
        return chunks
    
    # ═══════════════════════════════════════════════════════════════════════════
    # FULL PIPELINE
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def process_url(
        self,
        url: str,
        do_vad: bool = True,
    ) -> List[Path]:
        """
        Full pipeline: Download → Resample → VAD → Chunks
        
        Args:
            url: Audio URL
            do_vad: Có cắt chunk với VAD không
            
        Returns:
            List of chunk paths
        """
        print("\n" + "="*50)
        print("🎧 AUDIO PROCESSING")
        print("="*50)
        
        # Step 1: Download (already 16kHz Mono)
        audio_path = await self.download(url)
        
        # Step 2: VAD Chunking
        if do_vad:
            chunks = await self.vad_chunking(audio_path)
            
            # Cleanup original
            audio_path.unlink(missing_ok=True)
            
            return chunks
        else:
            return [audio_path]


# CLI for testing
if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="Audio Processor Test")
    parser.add_argument("--url", help="Audio URL to download and process")
    parser.add_argument("--input", help="Input audio file")
    parser.add_argument("--workspace", default="./workspace", help="Workspace dir")
    
    args = parser.parse_args()
    
    async def test():
        processor = AudioProcessor(Path(args.workspace))
        
        if args.url:
            chunks = await processor.process_url(args.url)
            print(f"\n✅ Created {len(chunks)} chunks")
            for chunk in chunks[:5]:
                print(f"   - {chunk}")
        
        elif args.input:
            audio_path = Path(args.input)
            info = processor.get_audio_info(audio_path)
            print(f"\n📊 Audio Info:")
            print(f"   Duration: {info.duration_sec:.1f}s")
            print(f"   Sample Rate: {info.sample_rate}Hz")
            print(f"   Channels: {info.channels}")
            
            # Test padding
            padded = processor.add_padding(audio_path)
            print(f"\n✅ Padded audio: {padded}")
            
            # Test VAD
            chunks = await processor.vad_chunking(padded)
            print(f"\n✅ Created {len(chunks)} VAD chunks")
    
    asyncio.run(test())
