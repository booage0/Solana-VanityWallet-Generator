use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
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

struct JobContext {
    prefix_bytes: Vec<u8>,
    pattern_rules: Option<Vec<PatternRule>>,
}

enum PatternKind {
    Single(u8),
    Sequence(Vec<u8>),
}

struct PatternRule {
    kind: PatternKind,
    min_length: usize,
}

impl JobContext {
    fn new(prefix: &str, config: Option<&Vec<PatternConfig>>) -> Self {
        Self {
            prefix_bytes: prefix.as_bytes().to_vec(),
            pattern_rules: preprocess_patterns(config),
        }
    }
}

fn preprocess_patterns(config: Option<&Vec<PatternConfig>>) -> Option<Vec<PatternRule>> {
    let patterns = match config {
        Some(p) => p,
        None => return None,
    };

    let mut rules = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        if pattern.pattern.is_empty() {
            continue;
        }

        let bytes = pattern.pattern.as_bytes();
        let kind = if bytes.len() == 1 {
            PatternKind::Single(bytes[0])
        } else {
            PatternKind::Sequence(bytes.to_vec())
        };

        rules.push(PatternRule {
            kind,
            min_length: pattern.min_length,
        });
    }

    if rules.is_empty() {
        None
    } else {
        Some(rules)
    }
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

fn find_rare_pattern(address_bytes: &[u8], job_context: &JobContext) -> Option<String> {
    let rules = match job_context.pattern_rules.as_ref() {
        Some(r) => r,
        None => return None,
    };

    for rule in rules {
        match &rule.kind {
            PatternKind::Single(target) => {
                let mut repeat_count = 0;
                for &byte in address_bytes {
                    if byte == *target {
                        repeat_count += 1;
                        continue;
                    }

                    if repeat_count >= rule.min_length {
                        let pattern = vec![*target; repeat_count];
                        if let Ok(found_pattern) = String::from_utf8(pattern) {
                            return Some(found_pattern);
                        }
                    }
                    repeat_count = 0;
                }

                if repeat_count >= rule.min_length {
                    let pattern = vec![*target; repeat_count];
                    if let Ok(found_pattern) = String::from_utf8(pattern) {
                        return Some(found_pattern);
                    }
                }
            }
            PatternKind::Sequence(pattern_bytes) => {
                let pattern_len = pattern_bytes.len();
                if pattern_len == 0 || address_bytes.len() < pattern_len * rule.min_length {
                    continue;
                }

                let mut index = 0;
                while index + pattern_len <= address_bytes.len() {
                    if &address_bytes[index..index + pattern_len] == pattern_bytes {
                        let mut match_count = 1;
                        let mut cursor = index + pattern_len;

                        while cursor + pattern_len <= address_bytes.len()
                            && &address_bytes[cursor..cursor + pattern_len] == pattern_bytes
                        {
                            match_count += 1;
                            cursor += pattern_len;
                        }

                        if match_count >= rule.min_length {
                            let mut repeated = Vec::with_capacity(pattern_len * match_count);
                            for _ in 0..match_count {
                                repeated.extend_from_slice(pattern_bytes);
                            }

                            if let Ok(found_pattern) = String::from_utf8(repeated) {
                                return Some(found_pattern);
                            }
                        }

                        index = cursor;
                        continue;
                    }

                    index += 1;
                }
            }
        }
    }

    None
}

fn encode_private_key(secret: &[u8; 32], public: &[u8; 32]) -> String {
    let mut keypair_bytes = [0u8; 64];
    keypair_bytes[..32].copy_from_slice(secret);
    keypair_bytes[32..].copy_from_slice(public);
    fd_bs58::encode_64(keypair_bytes)
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

        let job_context = Arc::new(JobContext::new(&prefix, config.as_ref()));
        let num_threads = num_cpus::get();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let file_lock = Arc::new(Mutex::new(()));
        let shared_attempts_counter = Arc::new(AtomicU64::new(0));
        let mut handles = vec![];

        for tid in 0..num_threads {
            let stop_flag_clone = Arc::clone(&stop_flag);
            let file_lock_clone = Arc::clone(&file_lock);
            let job_context_clone = Arc::clone(&job_context);
            let attempts_counter_clone = Arc::clone(&shared_attempts_counter);

            let handle = thread::spawn(move || {
                generate_vanity(
                    tid,
                    job_context_clone,
                    stop_flag_clone,
                    attempts_counter_clone,
                    file_lock_clone,
                )
            });

            handles.push(handle);
        }

        let mut last_report = Instant::now();

        loop {
            thread::sleep(Duration::from_millis(50));

            let now = Instant::now();
            if now.duration_since(last_report).as_millis() >= REPORT_INTERVAL_MS as u128 {
                let total_attempts = shared_attempts_counter.load(Ordering::Relaxed);
                let msg = OutputMessage::Progress { tid: 0, attempts: total_attempts };
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = writeln!(stdout, "{}", json);
                    let _ = stdout.flush();
                }
                last_report = now;
            }

            let all_done = handles.iter().all(|h| h.is_finished());
            if all_done {
                break;
            }
        }

        stop_flag.store(true, Ordering::Relaxed);

        for handle in handles {
            let _ = handle.join();
        }
    }
}

fn generate_vanity(
    _tid: usize,
    job_context: Arc<JobContext>,
    stop_flag: Arc<AtomicBool>,
    attempts_counter: Arc<AtomicU64>,
    file_lock: Arc<Mutex<()>>,
) {
    let mut rng = ChaCha20Rng::from_rng(OsRng).expect("Failed to seed RNG");
    let mut secret_bytes = [0u8; 32];
    let job_context_ref = job_context.as_ref();

    while !stop_flag.load(Ordering::Relaxed) {
        rand::RngCore::fill_bytes(&mut rng, &mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        
        let public_key = signing_key.verifying_key();
        let public_key_bytes = public_key.as_bytes();
        let address = fd_bs58::encode_32(public_key_bytes);
        let attempts = attempts_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let address_bytes = address.as_bytes();

        if let Some(pattern) = find_rare_pattern(address_bytes, job_context_ref) {
            let secret_bytes_key = signing_key.to_bytes();
            let private_key = encode_private_key(&secret_bytes_key, public_key_bytes);
            
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
                private_key,
                pattern,
                attempts,
            };

            if let Ok(json) = serde_json::to_string(&msg) {
                let mut stdout = io::stdout();
                let _ = writeln!(stdout, "{}", json);
                let _ = stdout.flush();
            }
        }

        if address_bytes.starts_with(&job_context_ref.prefix_bytes) {
            let secret_bytes_key = signing_key.to_bytes();
            let private_key = encode_private_key(&secret_bytes_key, public_key_bytes);
            
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

