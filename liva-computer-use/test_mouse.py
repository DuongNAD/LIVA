import sys
import os

# Add current directory to path
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

import controller
import time
import pyautogui

def test_mouse_corners():
    print("Starting Mouse Test in 3 seconds. Press F12 to abort.")
    listener = controller.start_panic_listener()
    time.sleep(3)
    
    screen_width, screen_height = pyautogui.size()
    
    try:
        # Top Left
        controller.move_mouse(10, 10)
        time.sleep(0.5)
        
        # Top Right
        controller.move_mouse(screen_width - 10, 10)
        time.sleep(0.5)
        
        # Bottom Right
        controller.move_mouse(screen_width - 10, screen_height - 10)
        time.sleep(0.5)
        
        # Bottom Left
        controller.move_mouse(10, screen_height - 10)
        time.sleep(0.5)
        
        # Center
        controller.move_mouse(screen_width // 2, screen_height // 2)
        
        print("Mouse test completed successfully.")
    except InterruptedError as e:
        print(f"Test aborted: {e}")
    finally:
        listener.stop()

if __name__ == "__main__":
    test_mouse_corners()
