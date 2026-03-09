use std::process::Command as StdCommand;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// JSON blokları ve kontrol tag'larını temizle
pub fn extract_speech_text(raw: &str) -> String {
    let mut result = String::new();
    let mut inside_code_block = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            inside_code_block = !inside_code_block;
            continue;
        }
        if inside_code_block { continue; }
        if trimmed.is_empty() { continue; }

        // Tag'ları satırdan sil, satırı atma
        let cleaned = trimmed
            .replace("<ENDCODE>", "")
            .replace("<EXECUTION_COMPLETE>", "");
        let cleaned = cleaned.trim();

        if !cleaned.is_empty() {
            result.push_str(cleaned);
            result.push(' ');
        }
    }
    result.trim().to_string()
}

fn mp3_path() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .join("oryxis_tts.mp3")
        .to_string_lossy()
        .to_string()
}

/// TTS: edge-tts ile sentezle, pygame ile oynat — tamamen Python tarafında
pub async fn speak(voice: &str, raw_response: &str) {
    eprintln!("[TTS] speak() entered, voice={}", voice);

    let text = extract_speech_text(raw_response);
    eprintln!("[TTS] extracted text: '{}'", &text[..text.len().min(80)]);
    if text.is_empty() {
        eprintln!("[TTS] text empty, returning");
        return;
    }

    let voice = voice.to_string();
    let out = mp3_path();
    eprintln!("[TTS] mp3 target: {}", out);

    let out_clone = out.clone();
    let text_clone = text.clone();
    let voice_clone = voice.clone();
    let handle = tokio::task::spawn_blocking(move || {
        // 1) Sentez
        eprintln!("[TTS] running: cmd /C edge-tts --voice {} --text [{}chars] --write-media {}", voice_clone, text_clone.len(), out_clone);

        let synth = StdCommand::new("cmd")
            .args(["/C", "edge-tts",
                   "--voice", &voice_clone,
                   "--text", &text_clone,
                   "--write-media", &out_clone])
            .output();

        match &synth {
            Ok(o) => {
                eprintln!("[TTS] edge-tts exit={}", o.status);
                if !o.stdout.is_empty() { eprintln!("[TTS] stdout: {}", String::from_utf8_lossy(&o.stdout)); }
                if !o.stderr.is_empty() { eprintln!("[TTS] stderr: {}", String::from_utf8_lossy(&o.stderr)); }
                if !o.status.success() { return; }
            }
            Err(e) => {
                eprintln!("[TTS] spawn error: {}", e);
                return;
            }
        }

        // mp3 kontrolü
        match std::fs::metadata(&out_clone) {
            Ok(m) => eprintln!("[TTS] mp3 exists, size={} bytes", m.len()),
            Err(e) => {
                eprintln!("[TTS] mp3 NOT found: {}", e);
                return;
            }
        }

        // 2) Oynat
        eprintln!("[TTS] playing via mci...");
        mci_play(&out_clone);
        eprintln!("[TTS] mci done");

        // 3) Temizle
        let _ = std::fs::remove_file(&out_clone);
    });

    if let Err(e) = handle.await {
        eprintln!("[TTS] task error: {}", e);
    }
}

#[cfg(windows)]
fn mci_play(path: &str) {
    use std::ffi::CString;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn mciSendStringA(cmd: *const i8, ret: *mut i8, ret_sz: u32, cb: usize) -> u32;
    }

    let path_clean = path.replace('/', "\\");
    let open = CString::new(format!(r#"open "{}" type mpegvideo alias otts"#, path_clean)).unwrap();
    let play = CString::new("play otts wait").unwrap();
    let close = CString::new("close otts").unwrap();

    unsafe {
        let r = mciSendStringA(open.as_ptr(), std::ptr::null_mut(), 0, 0);
        eprintln!("[TTS] mci open ret={}", r);
        if r != 0 { return; }
        let r = mciSendStringA(play.as_ptr(), std::ptr::null_mut(), 0, 0);
        eprintln!("[TTS] mci play ret={}", r);
        mciSendStringA(close.as_ptr(), std::ptr::null_mut(), 0, 0);
    }
}

#[cfg(not(windows))]
fn mci_play(_path: &str) {
    eprintln!("[TTS] mci only on Windows");
}