"""
Hallucination Filter - Anti-Hallucination cho Whisper

Features:
- WPS (Words Per Second) filter - Người Việt nói 1.5-4.0 từ/giây
- no_speech_prob filter - Skip silence/noise
- Pattern matching - Lọc ads, subtitles, common hallucinations
"""

import re
from dataclasses import dataclass
from typing import List, Optional


@dataclass
class TranscribedChunk:
    """Chunk đã transcribe"""
    audio_path: str
    text: str
    duration_sec: float
    no_speech_prob: float = 0.0
    words_per_second: float = 0.0
    is_valid: bool = True
    filter_reason: Optional[str] = None


class HallucinationFilter:
    """
    Bộ lọc ảo giác Whisper
    
    Whisper có thể tạo ra text từ:
    - Tiếng nhạc nền
    - Tiếng thở dài
    - Âm thanh môi trường
    - Subtitle/CC từ video
    
    Lọc bằng:
    1. WPS (Words Per Second) - Người Việt nói 1.5-4.0 từ/giây
    2. no_speech_prob - Xác suất không có tiếng nói
    3. Pattern matching - Các pattern ảo giác phổ biến
    """
    
    # Ngưỡng WPS cho tiếng Việt
    WPS_MIN = 1.0   # Người Việt nói chậm
    WPS_MAX = 5.0   # Người Việt nói nhanh
    
    # Ngưỡng no_speech_prob
    NO_SPEECH_THRESHOLD = 0.6
    
    # Pattern ảo giác phổ biến
    HALLUCINATION_PATTERNS = [
        # YouTube/Subtitle common
        r"cảm ơn các bạn đã theo dõi",
        r"nhấn đăng ký kênh",
        r"nhấn like và subscribe",
        r"subtitles?\s*by",
        r"please\s+subscribe",
        r"thank\s+you\s+for\s+watching",
        r"like\s+and\s+subscribe",
        r"hit\s+the\s+like\s+button",
        r"don'?t\s+forget\s+to",
        
        # Music/Audio artifacts
        r"♪[^♪]+♪",  # Lyrics with music notes
        r"🎵[^🎵]+🎵",
        r"\[music\]",
        r"\[applause\]",
        r"\[laughter\]",
        r"\[singing\]",
        
        # Common filler
        r"uhm+|uh+|um+|ah+|yeah",
        r"\b(mmm|mmm)\b",
        
        # Technical artifacts
        r"captions?\s*(by|are)?\s*(not\s+)?(verified|reviewed)",
        r"this\s+video\s+is\s+(sponsored|brought\s+to\s+you\s+by)",
    ]
    
    # Words that indicate hallucination (too many for duration)
    SUSPICIOUS_WORDS = [
        "subtitles", "subscribe", "follow", "like", "share",
        "comment", "notification", "bell", "youtube", "channel",
    ]
    
    def __init__(self):
        # Compile patterns
        self._patterns = [
            re.compile(p, re.IGNORECASE) 
            for p in self.HALLUCINATION_PATTERNS
        ]
    
    def filter(
        self,
        text: str,
        duration_sec: float,
        no_speech_prob: float = 0.0
    ) -> tuple[bool, Optional[str]]:
        """
        Kiểm tra chunk có phải là ảo giác không
        
        Args:
            text: Text đã transcribe
            duration_sec: Duration của audio (giây)
            no_speech_prob: Xác suất không có tiếng nói
            
        Returns:
            (is_valid, reason) - True nếu chunk hợp lệ
        """
        # Skip nếu text quá ngắn
        if len(text.strip()) < 2:
            return False, "text_too_short"
        
        # Skip nếu no_speech_prob cao
        if no_speech_prob > self.NO_SPEECH_THRESHOLD:
            return False, f"no_speech_prob_too_high ({no_speech_prob:.2f})"
        
        # Tính WPS
        word_count = len(text.split())
        wps = word_count / duration_sec if duration_sec > 0 else 0
        
        # Kiểm tra ngưỡng WPS
        if not (self.WPS_MIN <= wps <= self.WPS_MAX):
            return False, f"wps_out_of_range ({wps:.1f})"
        
        # Check patterns
        for pattern in self._patterns:
            if pattern.search(text):
                return False, f"hallucination_pattern ({pattern.pattern[:20]}...)"
        
        # Check suspicious words density
        suspicious_count = sum(
            1 for word in self.SUSPICIOUS_WORDS 
            if word.lower() in text.lower()
        )
        suspicious_ratio = suspicious_count / word_count if word_count > 0 else 0
        
        if suspicious_ratio > 0.2:  # More than 20% suspicious words
            return False, "too_many_suspicious_words"
        
        return True, None
    
    def filter_batch(
        self,
        chunks: List[dict]
    ) -> tuple[List[dict], List[dict]]:
        """
        Lọc nhiều chunks cùng lúc
        
        Args:
            chunks: List of dict với keys: audio_path, text, duration, no_speech_prob
            
        Returns:
            (valid_chunks, filtered_chunks)
        """
        valid = []
        filtered = []
        
        for chunk in chunks:
            is_valid, reason = self.filter(
                chunk.get('text', ''),
                chunk.get('duration', 1.0),
                chunk.get('no_speech_prob', 0.0)
            )
            
            chunk_result = {
                **chunk,
                'is_valid': is_valid,
                'filter_reason': reason,
            }
            
            if is_valid:
                valid.append(chunk_result)
            else:
                filtered.append(chunk_result)
        
        return valid, filtered
    
    def analyze_chunk(
        self,
        text: str,
        duration_sec: float,
        no_speech_prob: float = 0.0
    ) -> dict:
        """
        Phân tích chi tiết một chunk
        
        Returns:
            Dict với analysis results
        """
        word_count = len(text.split())
        wps = word_count / duration_sec if duration_sec > 0 else 0
        
        # Check each pattern
        matched_patterns = []
        for pattern in self._patterns:
            if pattern.search(text):
                matched_patterns.append(pattern.pattern)
        
        # Count suspicious words
        suspicious_found = [
            word for word in self.SUSPICIOUS_WORDS 
            if word.lower() in text.lower()
        ]
        
        return {
            'text': text,
            'word_count': word_count,
            'duration_sec': duration_sec,
            'wps': wps,
            'no_speech_prob': no_speech_prob,
            'wps_in_range': self.WPS_MIN <= wps <= self.WPS_MAX,
            'no_speech_ok': no_speech_prob <= self.NO_SPEECH_THRESHOLD,
            'matched_patterns': matched_patterns,
            'suspicious_words': suspicious_found,
            'is_valid': all([
                self.WPS_MIN <= wps <= self.WPS_MAX,
                no_speech_prob <= self.NO_SPEECH_THRESHOLD,
                not matched_patterns,
            ]),
        }
    
    @staticmethod
    def print_analysis(analysis: dict):
        """In kết quả phân tích"""
        print(f"\n📊 Chunk Analysis:")
        print(f"   Text: {analysis['text'][:50]}...")
        print(f"   Words: {analysis['word_count']}")
        print(f"   Duration: {analysis['duration_sec']:.1f}s")
        print(f"   WPS: {analysis['wps']:.2f} (range: {HallucinationFilter.WPS_MIN}-{HallucinationFilter.WPS_MAX})")
        print(f"   No speech prob: {analysis['no_speech_prob']:.2f}")
        
        if analysis['matched_patterns']:
            print(f"   ⚠️  Matched patterns: {analysis['matched_patterns']}")
        
        if analysis['suspicious_words']:
            print(f"   ⚠️  Suspicious words: {analysis['suspicious_words']}")
        
        status = "✅ VALID" if analysis['is_valid'] else "❌ FILTERED"
        print(f"   Status: {status}")


# CLI for testing
if __name__ == "__main__":
    # Test cases
    test_chunks = [
        # Valid Vietnamese
        {
            'audio_path': 'chunk_001.wav',
            'text': 'Hôm nay tôi đi ra ngoài chơi với bạn bè',
            'duration': 5.0,
            'no_speech_prob': 0.1,
        },
        # Too fast (likely hallucination)
        {
            'audio_path': 'chunk_002.wav',
            'text': 'cảm ơn các bạn đã theo dõi video hãy nhấn đăng ký kênh và like',
            'duration': 3.0,
            'no_speech_prob': 0.2,
        },
        # Low speech probability
        {
            'audio_path': 'chunk_003.wav',
            'text': 'có thể là gì đó',
            'duration': 5.0,
            'no_speech_prob': 0.8,
        },
        # Normal speed
        {
            'audio_path': 'chunk_004.wav',
            'text': 'Tôi rất vui được gặp bạn hôm nay',
            'duration': 4.0,
            'no_speech_prob': 0.05,
        },
    ]
    
    print("Hallucination Filter Test")
    print("=" * 50)
    
    filter = HallucinationFilter()
    
    valid, filtered = filter.filter_batch(test_chunks)
    
    print(f"\n📊 Results:")
    print(f"   Valid: {len(valid)} chunks")
    print(f"   Filtered: {len(filtered)} chunks")
    
    print(f"\n✅ Valid chunks:")
    for chunk in valid:
        print(f"   - {chunk['text'][:40]}... (WPS: {len(chunk['text'].split())/chunk['duration']:.1f})")
    
    print(f"\n❌ Filtered chunks:")
    for chunk in filtered:
        print(f"   - {chunk['text'][:40]}... ({chunk['filter_reason']})")
