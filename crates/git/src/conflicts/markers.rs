use std::io::{self, BufRead};

pub fn has_conflict_markers(bytes: &[u8]) -> bool {
    contains_markers(bytes).expect("an in-memory slice cannot fail to read")
}

/// Check line prefixes without loading a large working file into memory.
pub(super) fn contains_markers(mut reader: impl BufRead) -> io::Result<bool> {
    let mut marker = 0;
    let mut count = 0;
    let mut skip = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(count >= 7);
        }
        let length = buffer.len();
        for byte in buffer {
            if *byte == b'\n' {
                if count >= 7 {
                    return Ok(true);
                }
                marker = 0;
                count = 0;
                skip = false;
            } else if !skip {
                if count == 0 {
                    if matches!(byte, b'<' | b'>') {
                        marker = *byte;
                        count = 1;
                    } else {
                        skip = true;
                    }
                } else if *byte == marker {
                    count += 1;
                } else {
                    if count >= 7 && byte.is_ascii_whitespace() {
                        return Ok(true);
                    }
                    count = 0;
                    skip = true;
                }
            }
        }
        reader.consume(length);
    }
}
