use std::io::{IsTerminal, Write};

use chrono::NaiveDateTime;

/// Returns the current terminal width.
///
/// Priority:
/// 1. `COLUMNS` env var (explicit user/test override)
/// 2. 200 when stdout is not a terminal (piped)
/// 3. Actual terminal size via `terminal_size`
/// 4. Fallback: 80
pub fn term_width() -> usize {
    // COLUMNS env var takes priority (explicit user/test override)
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(n) = cols.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    // When piped (not a terminal), default to wide
    if !std::io::stdout().is_terminal() {
        return 200;
    }
    // Query actual terminal size
    if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
        if w > 0 {
            return w as usize;
        }
    }
    80
}

/// Parse a datetime string: replace space with T, if 10 chars add T00:00:00.
/// Returns a NaiveDateTime on success.
pub fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let normalized = s.replace(' ', "T");
    let full = if normalized.len() == 10 {
        format!("{}T00:00:00", normalized)
    } else if normalized.len() == 16 {
        // YYYY-MM-DDTHH:MM -> add :00
        format!("{}:00", normalized)
    } else {
        normalized
    };
    NaiveDateTime::parse_from_str(&full, "%Y-%m-%dT%H:%M:%S").ok()
}

/// Compute effective (from, to) date strings from the various date options.
pub fn compute_date_range(
    from_val: Option<String>,
    to_val: Option<String>,
    h5from_val: Option<String>,
    h5to_val: Option<String>,
    w1from_val: Option<String>,
    w1to_val: Option<String>,
) -> (Option<String>, Option<String>) {
    if let Some(ref val) = h5from_val {
        if let Some(dt) = parse_datetime(val) {
            let end = dt + chrono::Duration::hours(5);
            return (
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }

    if let Some(ref val) = h5to_val {
        if let Some(dt) = parse_datetime(val) {
            let start = dt - chrono::Duration::hours(5);
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }

    if let Some(ref val) = w1from_val {
        if let Some(dt) = parse_datetime(val) {
            let end = dt + chrono::Duration::days(7);
            return (
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }

    if let Some(ref val) = w1to_val {
        if let Some(dt) = parse_datetime(val) {
            let start = dt - chrono::Duration::days(7);
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }

    (from_val, to_val)
}

/// Map an output format name to its file extension.
pub fn ext_for_format(fmt: &str) -> &str {
    match fmt {
        "markdown" => "md",
        "json" => "json",
        "html" => "html",
        "txt" => "txt",
        "csv" => "csv",
        "tsv" => "tsv",
        _ => fmt,
    }
}

/// Base64-encode bytes (standard alphabet, with padding).
pub fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Copy text to clipboard via OSC 52 terminal escape sequence.
/// Works in most modern terminals without any external tools.
pub fn osc52_copy(text: &str) -> Result<(), String> {
    let encoded = base64_encode(text.as_bytes());
    // Write OSC 52 to stderr (which is typically the terminal)
    eprint!("\x1b]52;c;{}\x07", encoded);
    Ok(())
}

/// Copy text to the system clipboard.
/// Tries native clipboard tools first, falls back to OSC 52.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::process;

    let result = if cfg!(target_os = "macos") {
        process::Command::new("pbcopy")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
    } else if cfg!(target_os = "windows") {
        process::Command::new("clip")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
    } else {
        // Linux: try xclip, xsel, wl-copy in order
        process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
            .or_else(|_| {
                process::Command::new("xsel")
                    .arg("--clipboard")
                    .stdin(process::Stdio::piped())
                    .stdout(process::Stdio::null())
                    .stderr(process::Stdio::null())
                    .spawn()
            })
            .or_else(|_| {
                process::Command::new("wl-copy")
                    .stdin(process::Stdio::piped())
                    .stdout(process::Stdio::null())
                    .stderr(process::Stdio::null())
                    .spawn()
            })
    };

    match result {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
                drop(stdin); // Close stdin so the child sees EOF
            }
            let status = child
                .wait()
                .map_err(|e| format!("Clipboard process failed: {}", e))?;
            if status.success() {
                Ok(())
            } else {
                // Native tool failed, fall back to OSC 52
                osc52_copy(text)
            }
        }
        Err(_) => {
            // No native tool found, fall back to OSC 52
            osc52_copy(text)
        }
    }
}
