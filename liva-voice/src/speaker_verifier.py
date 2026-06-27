"""
Speaker Verifier - Xác minh speaker bằng SpeechBrain

Features:
- Load reference audio để lấy speaker embedding
- So sánh cosine similarity với các chunks
- Lọc bỏ giọng không phù hợp (similarity < threshold)
"""

import asyncio
import torch
import torchaudio
from pathlib import Path
from typing import Optional, List, Tuple
from dataclasses import dataclass

# Constants
SIMILARITY_THRESHOLD = 0.80  # 80% similarity tối thiểu
TARGET_SAMPLE_RATE = 16000


@dataclass
class SpeakerEmbedding:
    """Speaker embedding vector"""
    embedding: torch.Tensor
    audio_path: Path
    duration_sec: float


@dataclass
class VerificationResult:
    """Kết quả xác minh speaker"""
    audio_path: str
    similarity: float
    is_match: bool
    threshold: float


class SpeakerVerifier:
    """
    Speaker Verification sử dụng SpeechBrain ECAPA-TDNN
    
    Features:
    - Trích xuất embedding từ reference audio
    - So sánh cosine similarity với các chunks
    - Lọc bỏ giọng không phù hợp
    
    Usage:
        verifier = SpeakerVerifier()
        
        # Load reference
        ref_embedding = verifier.extract_embedding("reference.wav")
        
        # Verify chunks
        results = verifier.verify_chunks(chunks, ref_embedding)
        
        # Get matching chunks
        matches = [r for r in results if r.is_match]
    """
    
    def __init__(self, similarity_threshold: float = SIMILARITY_THRESHOLD):
        self.similarity_threshold = similarity_threshold
        self.encoder = None
        self._model_loaded = False
    
    # ═══════════════════════════════════════════════════════════════════════════
    # MODEL LOADING
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def load_model(self):
        """Load SpeechBrain encoder (lazy loading)"""
        if self._model_loaded:
            return
        
        print("    📦 Loading SpeechBrain ECAPA-TDNN encoder...")
        
        try:
            from speechbrain.inference.encoders import SpeakerEncoder
            
            self.encoder = SpeakerEncoder.from_hparams(
                source="speechbrain/spkrec-ecapa-voxceleb",
                savedir="pretrained_models/spkrec-ecapa-voxceleb",
                run_opts={"device": "cuda" if torch.cuda.is_available() else "cpu"}
            )
            
            self._model_loaded = True
            print("    ✅ SpeechBrain encoder loaded")
            
        except ImportError:
            print("    ⚠️  SpeechBrain not installed")
            print("    📝 Install with: pip install speechbrain")
            raise
    
    def load_model_sync(self):
        """Synchronous version của load_model"""
        if self._model_loaded:
            return
        
        print("    📦 Loading SpeechBrain ECAPA-TDNN encoder...")
        
        try:
            from speechbrain.inference.encoders import SpeakerEncoder
            
            self.encoder = SpeakerEncoder.from_hparams(
                source="speechbrain/spkrec-ecapa-voxceleb",
                savedir="pretrained_models/spkrec-ecapa-voxceleb",
                run_opts={"device": "cuda" if torch.cuda.is_available() else "cpu"}
            )
            
            self._model_loaded = True
            print("    ✅ SpeechBrain encoder loaded")
            
        except ImportError as e:
            print(f"    ⚠️  SpeechBrain not installed: {e}")
            print("    📝 Install with: pip install speechbrain")
            raise
    
    # ═══════════════════════════════════════════════════════════════════════════
    # EMBEDDING EXTRACTION
    # ═══════════════════════════════════════════════════════════════════════════
    
    def extract_embedding(self, audio_path: Path) -> SpeakerEmbedding:
        """
        Trích xuất speaker embedding từ audio
        
        Args:
            audio_path: Đường dẫn đến file audio
            
        Returns:
            SpeakerEmbedding object
        """
        if self.encoder is None:
            raise RuntimeError("Model not loaded. Call load_model() first.")
        
        # Load audio
        waveform, sr = torchaudio.load(str(audio_path))
        
        # Resample nếu cần
        if sr != TARGET_SAMPLE_RATE:
            resampler = torchaudio.transforms.Resample(sr, TARGET_SAMPLE_RATE)
            waveform = resampler(waveform)
        
        # Convert to mono
        if waveform.shape[0] > 1:
            waveform = waveform.mean(dim=0, keepdim=True)
        
        # Trích xuất embedding
        embedding = self.encoder.encode_batch(waveform)
        
        # Get duration
        duration = waveform.shape[1] / TARGET_SAMPLE_RATE
        
        return SpeakerEmbedding(
            embedding=embedding,
            audio_path=audio_path,
            duration_sec=duration
        )
    
    async def extract_embedding_async(self, audio_path: Path) -> SpeakerEmbedding:
        """Async version của extract_embedding"""
        return await asyncio.to_thread(self.extract_embedding, audio_path)
    
    # ═══════════════════════════════════════════════════════════════════════════
    # SIMILARITY COMPUTATION
    # ═══════════════════════════════════════════════════════════════════════════
    
    def compute_similarity(
        self,
        embedding1: torch.Tensor,
        embedding2: torch.Tensor
    ) -> float:
        """
        Tính cosine similarity giữa 2 embeddings
        
        Args:
            embedding1: Embedding thứ 1
            embedding2: Embedding thứ 2
            
        Returns:
            Cosine similarity (0.0 - 1.0)
        """
        # Flatten nếu cần
        e1 = embedding1.flatten()
        e2 = embedding2.flatten()
        
        # Cosine similarity
        similarity = torch.nn.functional.cosine_similarity(
            e1.unsqueeze(0), 
            e2.unsqueeze(0)
        ).item()
        
        return similarity
    
    def verify_chunk(
        self,
        chunk_path: Path,
        reference_embedding: SpeakerEmbedding
    ) -> VerificationResult:
        """
        Xác minh một chunk có giống reference speaker không
        
        Args:
            chunk_path: Đường dẫn đến chunk
            reference_embedding: Reference embedding
            
        Returns:
            VerificationResult
        """
        try:
            # Extract embedding từ chunk
            chunk_embedding = self.extract_embedding(chunk_path)
            
            # Tính similarity
            similarity = self.compute_similarity(
                reference_embedding.embedding,
                chunk_embedding.embedding
            )
            
            return VerificationResult(
                audio_path=str(chunk_path),
                similarity=similarity,
                is_match=similarity >= self.similarity_threshold,
                threshold=self.similarity_threshold
            )
            
        except Exception as e:
            print(f"    ⚠️  Error verifying {chunk_path.name}: {e}")
            return VerificationResult(
                audio_path=str(chunk_path),
                similarity=0.0,
                is_match=False,
                threshold=self.similarity_threshold
            )
    
    # ═══════════════════════════════════════════════════════════════════════════
    # BATCH VERIFICATION
    # ═══════════════════════════════════════════════════════════════════════════
    
    async def verify_chunks(
        self,
        chunks: List[Path],
        reference_audio: Path
    ) -> List[VerificationResult]:
        """
        Xác minh nhiều chunks với reference audio
        
        Args:
            chunks: Danh sách đường dẫn chunks
            reference_audio: Đường dẫn đến reference audio
            
        Returns:
            List of VerificationResult
        """
        if not chunks:
            return []
        
        # Load model
        await self.load_model()
        
        # Extract reference embedding
        print(f"    📊 Extracting reference embedding from {reference_audio.name}...")
        ref_embedding = await self.extract_embedding_async(reference_audio)
        print(f"    📊 Reference duration: {ref_embedding.duration_sec:.1f}s")
        
        # Verify each chunk
        results = []
        for i, chunk in enumerate(chunks):
            if i % 10 == 0:
                print(f"    🔄 [{i+1}/{len(chunks)}] Verifying chunks...")
            
            result = await asyncio.to_thread(
                self.verify_chunk, 
                chunk, 
                ref_embedding
            )
            results.append(result)
        
        # Count matches
        matches = [r for r in results if r.is_match]
        print(f"    📊 Matched: {len(matches)}/{len(chunks)} chunks "
              f"({len(matches)/len(chunks)*100:.1f}%)")
        
        return results
    
    def verify_chunks_sync(
        self,
        chunks: List[Path],
        reference_audio: Path
    ) -> List[VerificationResult]:
        """Synchronous version của verify_chunks"""
        if not chunks:
            return []
        
        # Load model
        self.load_model_sync()
        
        # Extract reference embedding
        print(f"    📊 Extracting reference embedding from {reference_audio.name}...")
        ref_embedding = self.extract_embedding(reference_audio)
        
        # Verify each chunk
        results = []
        for i, chunk in enumerate(chunks):
            if i % 10 == 0:
                print(f"    🔄 [{i+1}/{len(chunks)}] Verifying chunks...")
            
            result = self.verify_chunk(chunk, ref_embedding)
            results.append(result)
        
        # Count matches
        matches = [r for r in results if r.is_match]
        print(f"    📊 Matched: {len(matches)}/{len(chunks)} chunks")
        
        return results
    
    # ═══════════════════════════════════════════════════════════════════════════
    # FILTER CHUNKS
    # ═══════════════════════════════════════════════════════════════════════════
    
    def filter_chunks(
        self,
        chunks: List[Path],
        reference_audio: Path
    ) -> Tuple[List[Path], List[Path]]:
        """
        Lọc chunks theo speaker similarity
        
        Args:
            chunks: Danh sách chunks
            reference_audio: Reference audio
            
        Returns:
            (matching_chunks, filtered_chunks)
        """
        results = self.verify_chunks_sync(chunks, reference_audio)
        
        matching = []
        filtered = []
        
        for result in results:
            if result.is_match:
                matching.append(Path(result.audio_path))
            else:
                filtered.append(Path(result.audio_path))
                print(f"    ⚠️  Filtered (sim={result.similarity:.2f}): {Path(result.audio_path).name}")
        
        return matching, filtered


# CLI for testing
if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="Speaker Verifier Test")
    parser.add_argument("--reference", required=True, help="Reference audio file")
    parser.add_argument("--chunks-dir", required=True, help="Directory with chunks")
    
    args = parser.parse_args()
    
    verifier = SpeakerVerifier()
    
    # Get chunks
    chunks_dir = Path(args.chunks_dir)
    chunks = list(chunks_dir.glob("*.wav"))
    
    if not chunks:
        print(f"❌ No .wav files found in {chunks_dir}")
    else:
        print(f"📊 Found {len(chunks)} chunks")
        
        # Filter
        matching, filtered = verifier.filter_chunks(
            chunks, 
            Path(args.reference)
        )
        
        print(f"\n✅ Matching chunks: {len(matching)}")
        for chunk in matching[:5]:
            print(f"   - {chunk.name}")
        
        print(f"\n❌ Filtered chunks: {len(filtered)}")
