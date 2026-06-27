import os
import sys
import subprocess
import zipfile
import pytest

VERIFY_APK_SCRIPT = os.path.join(os.path.dirname(__file__), "verify_apk.py")
REAL_APK_PATH = r"E:\Project\LIVA\release\liva-mobile.apk"

def run_verification(apk_path):
    result = subprocess.run(
        [sys.executable, VERIFY_APK_SCRIPT, str(apk_path)],
        capture_output=True,
        text=True
    )
    return result

def test_non_existent_file(tmp_path):
    non_existent = tmp_path / "non_existent.apk"
    result = run_verification(non_existent)
    
    assert result.returncode == 1
    assert "Error: File does not exist" in result.stdout

def test_plain_text_file(tmp_path):
    text_file = tmp_path / "text.apk"
    text_file.write_text("This is just plain text content.")
    result = run_verification(text_file)
    
    assert result.returncode == 1
    assert "Error: File is not a valid zip archive" in result.stdout

def test_dummy_zip_missing_entries(tmp_path):
    dummy_zip = tmp_path / "missing_entries.apk"
    with zipfile.ZipFile(dummy_zip, 'w') as zf:
        zf.writestr("random_file.txt", "Some data")
        
    result = run_verification(dummy_zip)
    
    assert result.returncode == 1
    assert "Error: Missing standard Android app entries" in result.stdout
    assert "AndroidManifest.xml" in result.stdout

def test_dummy_zip_unsigned(tmp_path):
    dummy_zip = tmp_path / "unsigned.apk"
    with zipfile.ZipFile(dummy_zip, 'w') as zf:
        zf.writestr("AndroidManifest.xml", "<manifest></manifest>")
        zf.writestr("classes.dex", "dex data")
        zf.writestr("resources.arsc", "arsc data")
        
    result = run_verification(dummy_zip)
    
    assert result.returncode == 1
    assert "Error: APK is unsigned" in result.stdout

def test_dummy_zip_signed(tmp_path):
    dummy_zip = tmp_path / "signed.apk"
    with zipfile.ZipFile(dummy_zip, 'w') as zf:
        zf.writestr("AndroidManifest.xml", "<manifest></manifest>")
        zf.writestr("classes.dex", "dex data")
        zf.writestr("resources.arsc", "arsc data")
        zf.writestr("META-INF/CERT.SF", "signature file")
        zf.writestr("META-INF/CERT.RSA", "signature block")
        
    result = run_verification(dummy_zip)
    
    assert result.returncode == 0
    assert "Success: APK is a valid zip archive" in result.stdout

def test_real_apk():
    assert os.path.exists(REAL_APK_PATH), f"Real APK not found at: {REAL_APK_PATH}"
    result = run_verification(REAL_APK_PATH)
    
    assert result.returncode == 0
    assert "Success: APK is a valid zip archive" in result.stdout
    assert "Found signature files" in result.stdout
