import pyautogui
import time
from pynput import keyboard
import threading

# Configuration
pyautogui.FAILSAFE = True
pyautogui.PAUSE = 0.5  # Add a small delay between PyAutoGUI actions

# Global flag for the panic button
STOP_FLAG = False

def _on_press(key):
    global STOP_FLAG
    if key == keyboard.Key.f12:
        print("\n[CRITICAL] PANIC BUTTON (F12) PRESSED. STOPPING LIVA COMPUTER USE.")
        STOP_FLAG = True
        return False  # Stop listener

def start_panic_listener():
    """Starts a background thread listening for the F12 key to stop actions."""
    global STOP_FLAG
    STOP_FLAG = False
    listener = keyboard.Listener(on_press=_on_press)
    listener.start()
    return listener

def check_panic():
    """Raises an exception if the panic button was pressed."""
    if STOP_FLAG:
        raise InterruptedError("Action aborted by user (F12 pressed).")

def move_mouse(x, y, duration=0.5):
    """Moves the mouse to the specified coordinates smoothly."""
    check_panic()
    print(f"Action: Moving mouse to ({x}, {y})")
    pyautogui.moveTo(x, y, duration=duration, tween=pyautogui.easeInOutQuad)
    check_panic()

def click(x=None, y=None, clicks=1, button='left'):
    """Clicks the mouse at the current position or specified coordinates."""
    check_panic()
    if x is not None and y is not None:
        move_mouse(x, y)
    print(f"Action: Clicking {button} button {clicks} time(s)")
    pyautogui.click(clicks=clicks, button=button)
    check_panic()

def type_text(text, interval=0.05):
    """Types the specified text simulating keyboard strokes."""
    check_panic()
    print(f"Action: Typing text '{text}'")
    pyautogui.write(text, interval=interval)
    check_panic()

def press_key(key):
    """Presses a specific key (e.g., 'enter', 'tab')."""
    check_panic()
    print(f"Action: Pressing key '{key}'")
    pyautogui.press(key)
    check_panic()

def hotkey(*keys):
    """Presses a combination of hotkeys (e.g., 'ctrl', 'c')."""
    check_panic()
    print(f"Action: Pressing hotkey {keys}")
    pyautogui.hotkey(*keys)
    check_panic()
