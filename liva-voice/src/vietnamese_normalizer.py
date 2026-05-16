"""
Vietnamese Text Normalizer

Chuẩn hóa text tiếng Việt cho TTS:
- Số thành chữ (100 → "một trăm")
- Từ viết tắt (k → "nghìn", km → "ki lô mét")
- Từ ngoại lai (livestream → "lai sờ trim")
"""

import re
import subprocess
from typing import Optional

# Singleton instance
_normalizer: Optional['VietnameseNormalizer'] = None


def get_normalizer() -> 'VietnameseNormalizer':
    """Get singleton normalizer instance"""
    global _normalizer
    if _normalizer is None:
        _normalizer = VietnameseNormalizer()
    return _normalizer


class VietnameseNormalizer:
    """
    Vietnamese Text Normalizer
    
    Features:
    - Number to words (100 → "một trăm")
    - Abbreviation expansion (k → "nghìn")
    - Foreign word pronunciation (livestream → "lai sờ trim")
    - Phone number formatting
    - Percentage handling
    """
    
    def __init__(self):
        # Initialize num2words
        self._init_num2words()
        
        # Từ viết tắt thường gặp
        self.ABBREVIATIONS = {
            # Đơn vị đo lường
            "k": "nghìn",
            "km": "ki lô mét",
            "kg": "ki lô gam",
            "m": "mét",
            "cm": "xăng ti mét",
            "mm": "mi li mét",
            "ml": "mi li lít",
            "l": "lít",
            "gb": "gi ga bai",
            "mb": "mê ga bai",
            "kb": "ki lô bai",
            
            # Từ thông dụng
            "vn": "việt nam",
            "ai": "a i",
            "it": "i t",
            "bt": "bình thường",
            "vs": "với",
            "vip": "víp",
            "web": "wép",
            "app": "áp",
            "cpu": "si pi iu",
            "gpu": "gi pi iu",
            "ram": "ram",
            "hdd": "hạch đi",
            "ssd": "ét sờ đi",
            "usb": "i u ét bi",
            "hdmi": "hạch điêm ai",
            "vga": "vi gi ai",
            "dv": "dịch vụ",
            "pt": "phút",
            "g": "gam",
        }
        
        # Từ ngoại lai cần đọc theo cách Việt hóa
        self.FOREIGN_WORDS = {
            # Internet/Technology
            "livestream": "lai sờ trim",
            "live stream": "lai sờ trim",
            "online": "on lai",
            "offline": "oof lai",
            "download": "đao un lô",
            "upload": "áp lô",
            "wifi": "wai fai",
            "wi-fi": "wai fai",
            "bluetooth": "blu tút",
            "bluetooth": "blu tút",
            "chatgpt": "chát gí pí ti",
            "gpt": "gí pí ti",
            "youtube": "iu túp",
            "google": "gù gồ",
            "facebook": "phê iu bút",
            "twitter": "tuít tơ",
            "instagram": "in ét ta gram",
            "tiktok": "tí kếtốp",
            "zalo": "za lô",
            "messenger": "mê xen gơ",
            "zoom": "zum",
            
            # Software/Brand
            "windows": "win đớp",
            "macos": "mác ô s",
            "linux": "li nux",
            "android": "an đờ rôi",
            "ios": "ai ô és",
            "windows": "win đớp",
            
            # Đơn vị tiền tệ
            "usd": "đô la mỹ",
            "eur": "ê u rô",
            "gbp": "pon mỹ",
            "jpy": "yên nhật",
            "cny": "nhân dân tệ",
            
            # Khác
            "ok": "ô kê",
            "ceo": "si i ô",
            "cfo": "si ét ô",
            "cto": "si ti ô",
            "vip": "víp",
            "no1": "nôm bê oa",
        }
    
    def _init_num2words(self):
        """Initialize num2words library"""
        try:
            from num2words import num2words
            self._num2words = lambda n: num2words(n, lang='vi')
        except ImportError:
            print("    ⚠️  num2words not installed, installing...")
            subprocess.run(
                ["pip", "install", "num2words"],
                check=True,
                capture_output=True
            )
            from num2words import num2words
            self._num2words = lambda n: num2words(n, lang='vi')
    
    def normalize(self, text: str) -> str:
        """
        Chuẩn hóa một câu tiếng Việt
        
        Args:
            text: Text cần chuẩn hóa
            
        Returns:
            Text đã chuẩn hóa
        """
        if not text:
            return ""
        
        text = text.lower().strip()
        
        # 1. Xử lý số
        text = self._normalize_numbers(text)
        
        # 2. Xử lý từ viết tắt
        text = self._normalize_abbreviations(text)
        
        # 3. Xử lý từ ngoại lai
        text = self._normalize_foreign_words(text)
        
        # 4. Cleanup whitespace
        text = self._cleanup_whitespace(text)
        
        return text.strip()
    
    def _normalize_numbers(self, text: str) -> str:
        """Chuyển số thành chữ tiếng Việt"""
        
        # Số có đơn vị: 100k, 5kg, 2km, 50ml
        def replace_with_unit(match):
            num_str = match.group(1)
            unit = match.group(2)
            
            try:
                num = int(num_str)
                num_text = self._num2words(num)
            except:
                num_text = num_str
            
            unit_text = self.ABBREVIATIONS.get(unit, unit)
            return f"{num_text} {unit_text}"
        
        text = re.sub(
            r'(\d+)\s*(k|km|kg|m|cm|mm|ml|l|gb|mb|kb|g)',
            replace_with_unit,
            text
        )
        
        # Số điện thoại Việt Nam: 0901 234 567
        phone = r'(0\d{3,4})\s*(\d{3})\s*(\d{3,4})'
        text = re.sub(phone, r'\1 \2 \3', text)
        
        # Phần trăm: 50% → "năm mươi phần trăm"
        def replace_percent(match):
            num_str = match.group(1)
            try:
                num = int(num_str)
                return f"{self._num2words(num)} phần trăm"
            except:
                return match.group(0)
        
        text = re.sub(r'(\d+)%', replace_percent, text)
        
        # Số thập phân: 1.5 → "một phẩy năm"
        def replace_decimal(match):
            whole = match.group(1)
            decimal = match.group(2)
            try:
                whole_text = self._num2words(int(whole))
                decimal_text = ' '.join(self._num2words(int(d)) for d in decimal)
                return f"{whole_text} phẩy {decimal_text}"
            except:
                return match.group(0)
        
        text = re.sub(r'(\d+)\.(\d+)', replace_decimal, text)
        
        # Số trong ngoặc: (100) → "một trăm"
        def replace_in_parentheses(match):
            content = match.group(1)
            try:
                # Handle multiple numbers
                numbers = re.findall(r'\d+', content)
                if numbers:
                    result = content
                    for num_str in numbers:
                        try:
                            num = int(num_str)
                            num_text = self._num2words(num)
                            result = result.replace(num_str, num_text, 1)
                        except:
                            pass
                    return result
            except:
                pass
            return match.group(0)
        
        text = re.sub(r'\((\d+(?:\s*\d+)*)\)', replace_in_parentheses, text)
        
        return text
    
    def _normalize_abbreviations(self, text: str) -> str:
        """Xử lý từ viết tắt"""
        
        # Xử lý từ viết tắt có khoảng trắng 2 bên
        for abbr, full in self.ABBREVIATIONS.items():
            # Với khoảng trắng
            text = re.sub(rf'\s{re.escape(abbr)}\s', f' {full} ', text)
            # Với dấu câu
            text = re.sub(rf'\s{re.escape(abbr)}([.,!?;:])', rf' {full}\1', text)
        
        return text
    
    def _normalize_foreign_words(self, text: str) -> str:
        """Xử lý từ ngoại lai"""
        
        for word, pronounce in self.FOREIGN_WORDS.items():
            # Case insensitive replacement
            pattern = re.compile(re.escape(word), re.IGNORECASE)
            text = pattern.sub(pronounce, text)
        
        return text
    
    def _cleanup_whitespace(self, text: str) -> str:
        """Dọn dẹp khoảng trắng thừa"""
        
        # Multiple spaces to single space
        text = re.sub(r'\s+', ' ', text)
        
        # Spaces around punctuation
        text = re.sub(r'\s+([.,!?;:])', r'\1', text)
        text = re.sub(r'([.,!?;:])\s+', r'\1 ', text)
        
        return text
    
    def normalize_batch(self, texts: list[str]) -> list[str]:
        """Chuẩn hóa nhiều câu cùng lúc"""
        return [self.normalize(text) for text in texts]


# CLI for testing
if __name__ == "__main__":
    normalizer = VietnameseNormalizer()
    
    test_cases = [
        "Tôi có 100k trong tài khoản",
        "Khoảng cách 5km đó",
        "Cân nặng 70kg",
        "Livestream hôm nay rất vui",
        "Tôi ở Việt Nam",
        "Gọi cho tôi theo số 0901 234 567",
        "Độ ẩm 75% hôm nay",
        "Tốc độ internet 100mb",
        "Màn hình 15.6 inch",
        "OK, tôi đồng ý",
    ]
    
    print("Vietnamese Normalizer Test")
    print("=" * 50)
    
    for text in test_cases:
        normalized = normalizer.normalize(text)
        print(f"📝 {text}")
        print(f"   → {normalized}")
        print()
