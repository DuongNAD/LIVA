"""
LIVA 2.0 - Voice Cloning Pipeline

Export main classes for easy import.
"""

from .vram_manager import VRAMManager, GPULockContext, get_vram_manager
from .audio_processor import AudioProcessor, AudioInfo
from .hallucination_filter import HallucinationFilter, TranscribedChunk
from .speaker_verifier import SpeakerVerifier, SpeakerEmbedding, VerificationResult
from .vietnamese_normalizer import VietnameseNormalizer, get_normalizer
from .gpt_sovits_core import GPTSoVITSCore, TrainingConfig, TrainingProgress
from .voice_pipeline import VoicePipeline, CloneResult, PipelineStats

__all__ = [
    # VRAM Manager
    "VRAMManager",
    "GPULockContext", 
    "get_vram_manager",
    
    # Audio Processor
    "AudioProcessor",
    "AudioInfo",
    
    # Hallucination Filter
    "HallucinationFilter",
    "TranscribedChunk",
    
    # Speaker Verifier
    "SpeakerVerifier",
    "SpeakerEmbedding",
    "VerificationResult",
    
    # Vietnamese Normalizer
    "VietnameseNormalizer",
    "get_normalizer",
    
    # GPT-SoVITS Core
    "GPTSoVITSCore",
    "TrainingConfig",
    "TrainingProgress",
    
    # Main Pipeline
    "VoicePipeline",
    "CloneResult",
    "PipelineStats",
]

__version__ = "2.0.0"
__author__ = "LIVA Team"
