use super::hook::RawEvent;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct FlushedPayload {
    pub window_title: String,
    pub process_name: String,
    pub text: String,
    pub timestamp: u64,
}

pub struct ActiveSessionBuffer {
    accumulated_text: String,
    current_window_title: String,
    current_process_name: String,
    last_activity: Instant,
    length_threshold: usize,
}

impl ActiveSessionBuffer {
    pub fn new(length_threshold: usize) -> Self {
        Self {
            accumulated_text: String::new(),
            current_window_title: String::new(),
            current_process_name: String::new(),
            last_activity: Instant::now(),
            length_threshold,
        }
    }

    fn flush(&mut self) -> Option<FlushedPayload> {
        if self.accumulated_text.is_empty() {
            return None;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let payload = FlushedPayload {
            window_title: self.current_window_title.clone(),
            process_name: self.current_process_name.clone(),
            text: self.accumulated_text.clone(),
            timestamp,
        };
        self.accumulated_text.clear();
        Some(payload)
    }

    pub fn add_event(&mut self, event: RawEvent) -> Option<FlushedPayload> {
        let mut flushed = None;

        let (ev_window_title, ev_process_name) = match &event {
            RawEvent::KeyPress {
                window_title,
                process_name,
                ..
            } => (window_title.clone(), process_name.clone()),
            RawEvent::MouseClick {
                window_title,
                process_name,
                ..
            } => (window_title.clone(), process_name.clone()),
        };

        // Rule 1: Window Switch
        if !self.accumulated_text.is_empty()
            && (self.current_window_title != ev_window_title
                || self.current_process_name != ev_process_name)
        {
            flushed = self.flush();
        }

        if self.accumulated_text.is_empty() {
            self.current_window_title = ev_window_title;
            self.current_process_name = ev_process_name;
        }

        self.last_activity = Instant::now();

        match event {
            RawEvent::KeyPress { key, vk_code, .. } => {
                let mut should_flush_after = false;

                if vk_code == 0x0D {
                    // Enter
                    self.accumulated_text.push_str(" [Enter] ");
                    should_flush_after = true;
                } else if vk_code == 0x09 {
                    // Tab
                    self.accumulated_text.push_str(" [Tab] ");
                    should_flush_after = true;
                } else if vk_code == 0x08 {
                    // Backspace
                    self.accumulated_text.pop();
                } else {
                    self.accumulated_text.push_str(&key);
                }

                if self.accumulated_text.len() >= self.length_threshold {
                    if flushed.is_none() {
                        flushed = self.flush();
                    }
                } else if should_flush_after {
                    if flushed.is_none() {
                        flushed = self.flush();
                    }
                }
            }
            RawEvent::MouseClick { button, x, y, .. } => {
                self.accumulated_text
                    .push_str(&format!(" [Click:{}({},{})] ", button, x, y));

                if self.accumulated_text.len() >= self.length_threshold {
                    if flushed.is_none() {
                        flushed = self.flush();
                    }
                }
            }
        }

        flushed
    }

    pub fn check_timeout(&mut self, timeout: std::time::Duration) -> Option<FlushedPayload> {
        if self.last_activity.elapsed() >= timeout {
            self.flush()
        } else {
            None
        }
    }

    pub fn get_accumulated_text(&self) -> &str {
        &self.accumulated_text
    }

    pub fn get_current_window_title(&self) -> &str {
        &self.current_window_title
    }

    pub fn get_current_process_name(&self) -> &str {
        &self.current_process_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_keystroke_aggregation() {
        let mut buffer = ActiveSessionBuffer::new(100);
        let event1 = RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        };
        let event2 = RawEvent::KeyPress {
            key: "b".to_string(),
            vk_code: 66,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        };

        assert!(buffer.add_event(event1).is_none());
        assert!(buffer.add_event(event2).is_none());
        assert_eq!(buffer.get_accumulated_text(), "ab");
        assert_eq!(buffer.get_current_window_title(), "Notepad");
        assert_eq!(buffer.get_current_process_name(), "notepad.exe");
    }

    #[test]
    fn test_backspace_logic() {
        let mut buffer = ActiveSessionBuffer::new(100);
        buffer.add_event(RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });
        buffer.add_event(RawEvent::KeyPress {
            key: "b".to_string(),
            vk_code: 66,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });
        buffer.add_event(RawEvent::KeyPress {
            key: "".to_string(),
            vk_code: 0x08, // Backspace
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        assert_eq!(buffer.get_accumulated_text(), "a");
    }

    #[test]
    fn test_window_switch_flush() {
        let mut buffer = ActiveSessionBuffer::new(100);
        buffer.add_event(RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        // Switch window
        let flushed = buffer.add_event(RawEvent::KeyPress {
            key: "x".to_string(),
            vk_code: 88,
            window_title: "Chrome".to_string(),
            process_name: "chrome.exe".to_string(),
        });

        assert!(flushed.is_some());
        let payload = flushed.unwrap();
        assert_eq!(payload.window_title, "Notepad");
        assert_eq!(payload.process_name, "notepad.exe");
        assert_eq!(payload.text, "a");

        assert_eq!(buffer.get_accumulated_text(), "x");
        assert_eq!(buffer.get_current_window_title(), "Chrome");
        assert_eq!(buffer.get_current_process_name(), "chrome.exe");
    }

    #[test]
    fn test_delimiter_flush() {
        let mut buffer = ActiveSessionBuffer::new(100);
        buffer.add_event(RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        // Press Enter
        let flushed = buffer.add_event(RawEvent::KeyPress {
            key: "".to_string(),
            vk_code: 0x0D, // Enter
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        assert!(flushed.is_some());
        let payload = flushed.unwrap();
        assert_eq!(payload.text, "a [Enter] ");
        assert_eq!(buffer.get_accumulated_text(), "");
    }

    #[test]
    fn test_length_threshold_flush() {
        let mut buffer = ActiveSessionBuffer::new(5);
        buffer.add_event(RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });
        buffer.add_event(RawEvent::KeyPress {
            key: "b".to_string(),
            vk_code: 66,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });
        buffer.add_event(RawEvent::KeyPress {
            key: "c".to_string(),
            vk_code: 67,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });
        buffer.add_event(RawEvent::KeyPress {
            key: "d".to_string(),
            vk_code: 68,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        // Next character triggers length >= 5
        let flushed = buffer.add_event(RawEvent::KeyPress {
            key: "e".to_string(),
            vk_code: 69,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        assert!(flushed.is_some());
        assert_eq!(flushed.unwrap().text, "abcde");
        assert_eq!(buffer.get_accumulated_text(), "");
    }

    #[test]
    fn test_inactivity_timeout_flush() {
        let mut buffer = ActiveSessionBuffer::new(100);
        buffer.add_event(RawEvent::KeyPress {
            key: "a".to_string(),
            vk_code: 65,
            window_title: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
        });

        // Fast check, no timeout
        assert!(
            buffer
                .check_timeout(std::time::Duration::from_secs(10))
                .is_none()
        );

        // We manually shift the last_activity back in time for testing
        buffer.last_activity = Instant::now() - std::time::Duration::from_secs(15);
        let flushed = buffer.check_timeout(std::time::Duration::from_secs(10));
        assert!(flushed.is_some());
        assert_eq!(flushed.unwrap().text, "a");
        assert_eq!(buffer.get_accumulated_text(), "");
    }

    #[test]
    fn test_hello_liva_simulation() {
        let mut buffer = ActiveSessionBuffer::new(100);
        let text_to_type = "Hello LIVA";
        for c in text_to_type.chars() {
            let event = RawEvent::KeyPress {
                key: c.to_string(),
                vk_code: c as u32,
                window_title: "Notepad".to_string(),
                process_name: "notepad.exe".to_string(),
            };
            buffer.add_event(event);
        }
        assert_eq!(buffer.get_accumulated_text(), "Hello LIVA");
    }
}
