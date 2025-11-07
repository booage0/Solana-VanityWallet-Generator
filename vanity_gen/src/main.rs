use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Serialize)]
#[serde(tag = "type")]
enum OutputMessage {
    #[serde(rename = "progress")]
    Progress { tid: usize, attempts: u64 },
    #[serde(rename = "found")]
    Found {
        address: String,
        private_key: String,
        attempts: u64,
    },
    #[serde(rename = "rare")]
    Rare {
        address: String,
        private_key: String,
        pattern: String,
        attempts: u64,
    },
}

#[derive(Deserialize)]
struct InputMessage {
    prefix: Option<String>,
}

const REPORT_INTERVAL_MS: u64 = 250;

#[derive(Deserialize, Clone)]
struct PatternConfig {
    pattern: String,
    #[serde(rename = "minLength")]
    min_length: usize,
}

#[derive(Deserialize)]
struct Config {
    patterns: Vec<PatternConfig>,
}

fn load_config() -> Option<Vec<PatternConfig>> {
    let config_paths = vec![
        PathBuf::from("config.json"),
        PathBuf::from("vanity_gen").join("config.json"),
    ];

    for path in config_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<Config>(&content) {
                    return Some(config.patterns);
                }
            }
        }
    }

    None
}

fn is_rare_pattern(address: &str, config: &Option<Vec<PatternConfig>>) -> Option<String> {
    let patterns = match config {
        Some(p) => p,
        None => return None,
    };

    let chars: Vec<char> = address.chars().collect();
    if chars.len() < 2 {
        return None;
    }

    for pattern_config in patterns {
        let pattern_chars: Vec<char> = pattern_config.pattern.chars().collect();
        
        if pattern_chars.is_empty() {
            continue;
        }

        if pattern_chars.len() == 1 {
            let target_char = pattern_chars[0];
            let mut repeat_count = 0;
            
            for &ch in &chars {
                if ch == target_char {
                    repeat_count += 1;
                } else {
                    if repeat_count >= pattern_config.min_length {
                        let found_pattern: String = std::iter::repeat(target_char).take(repeat_count).collect();
                        return Some(found_pattern);
                    }
                    repeat_count = 0;
                }
            }
            
            if repeat_count >= pattern_config.min_length {
                let found_pattern: String = std::iter::repeat(target_char).take(repeat_count).collect();
                return Some(found_pattern);
            }
        } else {
            let pattern_len = pattern_chars.len();
            let min_chars_needed = pattern_len * pattern_config.min_length;
            
            if chars.len() < min_chars_needed {
                continue;
            }
            
            for i in 0..=chars.len().saturating_sub(min_chars_needed) {
                let mut match_count = 0;
                let mut j = i;
                
                while j + pattern_len <= chars.len() {
                    let mut matches = true;
                    for k in 0..pattern_len {
                        if chars[j + k] != pattern_chars[k] {
                            matches = false;
                            break;
                        }
                    }
                    
                    if matches {
                        match_count += 1;
                        j += pattern_len;
                    } else {
                        break;
                    }
                }
                
                if match_count >= pattern_config.min_length {
                    let found_pattern: String = pattern_config.pattern.repeat(match_count);
                    return Some(found_pattern);
                }
            }
        }
    }

    None
}

fn main() {
    let config = load_config();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let line_trimmed = line.trim();
        if line_trimmed == "stop" {
            break;
        }

        let input: InputMessage = match serde_json::from_str(&line_trimmed) {
            Ok(msg) => msg,
            Err(_) => continue,
        };

        let prefix = match input.prefix {
            Some(p) => p,
            None => continue,
        };

        let num_threads = num_cpus::get();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let file_lock = Arc::new(Mutex::new(()));
        let mut handles = vec![];

        for tid in 0..num_threads {
            let prefix_clone = prefix.clone();
            let stop_flag_clone = Arc::clone(&stop_flag);
            let file_lock_clone = Arc::clone(&file_lock);
            let config_clone = config.clone();
            let attempts_counter = Arc::new(AtomicU64::new(0));
            let attempts_counter_clone = Arc::clone(&attempts_counter);

            let handle = thread::spawn(move || {
                generate_vanity(tid, &prefix_clone, &stop_flag_clone, &attempts_counter_clone, &file_lock_clone, &config_clone)
            });

            handles.push((handle, attempts_counter));
        }

        let mut last_report = Instant::now();

        loop {
            thread::sleep(Duration::from_millis(50));

            let now = Instant::now();
            if now.duration_since(last_report).as_millis() >= REPORT_INTERVAL_MS as u128 {
                for (tid, attempts_counter) in handles.iter().enumerate() {
                    let attempts = attempts_counter.1.load(Ordering::Relaxed);
                    let msg = OutputMessage::Progress { tid, attempts };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = writeln!(stdout, "{}", json);
                        let _ = stdout.flush();
                    }
                }
                last_report = now;
            }

            let all_done = handles.iter().all(|h| h.0.is_finished());
            if all_done {
                break;
            }
        }

        stop_flag.store(true, Ordering::Relaxed);

        for (handle, _) in handles {
            let _ = handle.join();
        }
    }
}

fn generate_vanity(
    _tid: usize,
    prefix: &str,
    stop_flag: &Arc<AtomicBool>,
    attempts_counter: &Arc<AtomicU64>,
    file_lock: &Arc<Mutex<()>>,
    config: &Option<Vec<PatternConfig>>,
) {
    let mut rng = OsRng;
    let mut attempts: u64 = 0;

    while !stop_flag.load(Ordering::Relaxed) {
        let mut secret_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rng, &mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        
        attempts += 1;
        attempts_counter.store(attempts, Ordering::Relaxed);

        let public_key = signing_key.verifying_key();
        let public_key_bytes = public_key.as_bytes();
        let address = bs58::encode(public_key_bytes).into_string();
        
        let secret_bytes_key = signing_key.to_bytes();
        let public_bytes = public_key_bytes;
        let mut keypair_bytes = [0u8; 64];
        keypair_bytes[..32].copy_from_slice(&secret_bytes_key);
        keypair_bytes[32..].copy_from_slice(public_bytes);
        let private_key = bs58::encode(keypair_bytes).into_string();

        if let Some(pattern) = is_rare_pattern(&address, config) {
            let _lock = file_lock.lock().unwrap();
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("rare_wallets.txt")
            {
                let _ = writeln!(file, "Pattern: {}\nAddress: {}\nPrivate Key: {}\n", pattern, address, private_key);
            }
            drop(_lock);
            
            let msg = OutputMessage::Rare {
                address: address.clone(),
                private_key: private_key.clone(),
                pattern,
                attempts,
            };

            if let Ok(json) = serde_json::to_string(&msg) {
                let mut stdout = io::stdout();
                let _ = writeln!(stdout, "{}", json);
                let _ = stdout.flush();
            }
        }

        if address.to_lowercase().starts_with(&prefix.to_lowercase()) {
            let msg = OutputMessage::Found {
                address,
                private_key,
                attempts,
            };

            if let Ok(json) = serde_json::to_string(&msg) {
                let mut stdout = io::stdout();
                let _ = writeln!(stdout, "{}", json);
                let _ = stdout.flush();
            }

            stop_flag.store(true, Ordering::Relaxed);
            break;
        }
    }
}

