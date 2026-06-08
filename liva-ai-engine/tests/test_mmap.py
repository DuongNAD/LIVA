import os
import sys
from liva_native_engine import is_macos_memory_pressure, should_use_mmap


def test_is_macos_memory_pressure():
    res = is_macos_memory_pressure()
    assert isinstance(res, bool)
    if sys.platform != "darwin":
        assert res is False


def test_should_use_mmap_env_override():
    # Test True override
    os.environ["NATIVE_USE_MMAP"] = "true"
    assert should_use_mmap() is True
    
    os.environ["NATIVE_USE_MMAP"] = "1"
    assert should_use_mmap() is True

    # Test False override
    os.environ["NATIVE_USE_MMAP"] = "false"
    assert should_use_mmap() is False

    os.environ["NATIVE_USE_MMAP"] = "0"
    assert should_use_mmap() is False

    # Clean up
    del os.environ["NATIVE_USE_MMAP"]


def test_should_use_mmap_default():
    if "NATIVE_USE_MMAP" in os.environ:
        del os.environ["NATIVE_USE_MMAP"]
        
    res = should_use_mmap()
    assert isinstance(res, bool)
    if sys.platform == "darwin":
        # On macOS, it should equal not is_macos_memory_pressure()
        assert res == (not is_macos_memory_pressure())
    else:
        # On other platforms, it defaults to True
        assert res is True
