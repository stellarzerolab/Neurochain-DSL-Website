use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402StellarChallenge {
    pub created_at: u64,
    pub expires_at: u64,
    pub finalized: bool,
    pub finalized_at: Option<u64>,
    pub payment_state: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct X402StellarState {
    next_id: u64,
    #[serde(default)]
    challenges: HashMap<String, X402StellarChallenge>,
    #[serde(default)]
    used_challenges: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct X402ChallengeRecord {
    pub challenge_id: String,
    pub challenge: X402StellarChallenge,
}

#[derive(Debug, Clone)]
pub enum X402FinalizeOutcome {
    Finalized(X402StellarChallenge),
    ReplayBlocked(X402StellarChallenge),
    Expired(X402StellarChallenge),
    UnknownChallenge,
}

pub trait X402ChallengeStore {
    fn store_kind(&self) -> &'static str;
    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String>;
    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String>;
}

impl X402StellarState {
    fn create_challenge(&mut self) -> X402ChallengeRecord {
        self.next_id += 1;
        let challenge_id = format!("x402s{:04}", self.next_id);
        let created_at = now_unix_secs();
        let expires_at = created_at.saturating_add(x402_stellar_ttl_secs());
        let challenge = X402StellarChallenge {
            created_at,
            expires_at,
            finalized: false,
            finalized_at: None,
            payment_state: "payment_required".to_string(),
        };
        self.challenges
            .insert(challenge_id.clone(), challenge.clone());
        X402ChallengeRecord {
            challenge_id,
            challenge,
        }
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> X402FinalizeOutcome {
        let used = self.used_challenges.contains(challenge_id);
        let Some(challenge) = self.challenges.get_mut(challenge_id) else {
            return X402FinalizeOutcome::UnknownChallenge;
        };

        if used || challenge.finalized {
            challenge.payment_state = "replay_blocked".to_string();
            return X402FinalizeOutcome::ReplayBlocked(challenge.clone());
        }

        if now_unix_secs() >= challenge.expires_at {
            challenge.payment_state = "expired".to_string();
            return X402FinalizeOutcome::Expired(challenge.clone());
        }

        let finalized_at = now_unix_secs();
        challenge.finalized = true;
        challenge.finalized_at = Some(finalized_at);
        challenge.payment_state = "finalized".to_string();
        self.used_challenges.insert(challenge_id.to_string());
        X402FinalizeOutcome::Finalized(challenge.clone())
    }
}

#[derive(Debug, Default)]
struct InMemoryX402ChallengeStore {
    state: X402StellarState,
}

#[derive(Debug)]
struct UnavailableX402ChallengeStore {
    error: String,
}

impl X402ChallengeStore for InMemoryX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "in_memory"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        Ok(self.state.create_challenge())
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        Ok(self.state.begin_finalize(challenge_id))
    }
}

impl X402ChallengeStore for UnavailableX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "unavailable"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        Err(self.error.clone())
    }

    fn begin_finalize(&mut self, _challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        Err(self.error.clone())
    }
}

#[derive(Debug)]
struct FileX402ChallengeStore {
    path: PathBuf,
    state: X402StellarState,
}

impl FileX402ChallengeStore {
    fn load(path: PathBuf) -> Result<Self, String> {
        let state = match fs::read_to_string(&path) {
            Ok(raw) if raw.trim().is_empty() => X402StellarState::default(),
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|err| format!("x402 store parse failed at {}: {err}", path.display()))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => X402StellarState::default(),
            Err(err) => {
                return Err(format!(
                    "x402 store read failed at {}: {err}",
                    path.display()
                ));
            }
        };

        Ok(Self { path, state })
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "x402 store mkdir failed at {}: {err}",
                    parent.to_string_lossy()
                )
            })?;
        }

        let raw = serde_json::to_string_pretty(&self.state)
            .map_err(|err| format!("x402 store serialize failed: {err}"))?;
        let tmp_path = self.path.with_extension("json.tmp");
        fs::write(&tmp_path, raw)
            .map_err(|err| format!("x402 store write failed at {}: {err}", tmp_path.display()))?;
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|err| {
                format!(
                    "x402 store replace failed at {}: {err}",
                    self.path.display()
                )
            })?;
        }
        fs::rename(&tmp_path, &self.path).map_err(|err| {
            format!(
                "x402 store rename failed from {} to {}: {err}",
                tmp_path.display(),
                self.path.display()
            )
        })
    }
}

impl X402ChallengeStore for FileX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "file"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        let record = self.state.create_challenge();
        self.persist()?;
        Ok(record)
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        let outcome = self.state.begin_finalize(challenge_id);
        if !matches!(outcome, X402FinalizeOutcome::UnknownChallenge) {
            self.persist()?;
        }
        Ok(outcome)
    }
}

pub fn build_x402_challenge_store() -> Box<dyn X402ChallengeStore + Send> {
    let Some(path) = x402_stellar_store_path() else {
        return Box::<InMemoryX402ChallengeStore>::default();
    };

    match FileX402ChallengeStore::load(path.clone()) {
        Ok(store) => Box::new(store),
        Err(err) => {
            eprintln!("ERROR: {err}; x402 challenge store unavailable");
            Box::new(UnavailableX402ChallengeStore { error: err })
        }
    }
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn x402_stellar_ttl_secs() -> u64 {
    env::var("NC_X402_STELLAR_TTL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(300)
}

fn x402_stellar_store_path() -> Option<PathBuf> {
    env::var("NC_X402_STELLAR_STORE_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
