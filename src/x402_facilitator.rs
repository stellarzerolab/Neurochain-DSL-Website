use std::env;

use crate::x402_store::{
    X402ChallengeRecord, X402ChallengeStore, X402FinalizeOutcome, X402StellarChallenge,
};

#[derive(Debug, Clone)]
pub enum X402PaymentVerification {
    Finalized {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    ReplayBlocked {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    Expired {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    InvalidPayment,
}

pub trait X402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str;
    fn boundary_kind(&self) -> &'static str;
    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String>;
    fn verify_and_finalize(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String>;
}

#[derive(Debug, Default)]
struct MockX402PaymentVerifier;

#[derive(Debug, Default)]
struct FacilitatorX402PaymentVerifier;

#[derive(Debug)]
struct UnavailableX402PaymentVerifier {
    reason: String,
}

impl X402PaymentVerifier for MockX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "mock"
    }

    fn boundary_kind(&self) -> &'static str {
        "mock_header_store"
    }

    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        store.create_challenge()
    }

    fn verify_and_finalize(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        let Some(challenge_id) =
            mock_challenge_from_signature(payment_signature).map(str::to_string)
        else {
            return Ok(X402PaymentVerification::InvalidPayment);
        };

        let verification = match store.begin_finalize(&challenge_id)? {
            X402FinalizeOutcome::Finalized(challenge) => X402PaymentVerification::Finalized {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::ReplayBlocked(challenge) => {
                X402PaymentVerification::ReplayBlocked {
                    challenge_id,
                    challenge,
                }
            }
            X402FinalizeOutcome::Expired(challenge) => X402PaymentVerification::Expired {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::UnknownChallenge => X402PaymentVerification::InvalidPayment,
        };

        Ok(verification)
    }
}

impl X402PaymentVerifier for FacilitatorX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "facilitator"
    }

    fn boundary_kind(&self) -> &'static str {
        "facilitator_verify_settle"
    }

    fn create_challenge(
        &self,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        Err(facilitator_transport_unavailable())
    }

    fn verify_and_finalize(
        &self,
        _payment_signature: &str,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        Err(facilitator_transport_unavailable())
    }
}

impl X402PaymentVerifier for UnavailableX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "unavailable"
    }

    fn boundary_kind(&self) -> &'static str {
        "facilitator_required"
    }

    fn create_challenge(
        &self,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        Err(self.reason.clone())
    }

    fn verify_and_finalize(
        &self,
        _payment_signature: &str,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        Err(self.reason.clone())
    }
}

pub fn build_x402_payment_verifier() -> Box<dyn X402PaymentVerifier + Send + Sync> {
    let mode = env::var("NC_X402_STELLAR_VERIFIER")
        .unwrap_or_else(|_| "mock".to_string())
        .trim()
        .to_ascii_lowercase();

    match mode.as_str() {
        "mock" if x402_runtime_is_production() => Box::new(UnavailableX402PaymentVerifier {
            reason:
                "mock x402 verifier is disabled in production; configure the facilitator verifier"
                    .to_string(),
        }),
        "mock" => Box::<MockX402PaymentVerifier>::default(),
        "facilitator" => Box::<FacilitatorX402PaymentVerifier>::default(),
        _ => Box::new(UnavailableX402PaymentVerifier {
            reason: format!(
                "unsupported x402 verifier mode {mode:?}; expected \"mock\" or \"facilitator\""
            ),
        }),
    }
}

fn facilitator_transport_unavailable() -> String {
    "facilitator x402 verifier is selected, but verify/settle transport is not implemented"
        .to_string()
}

fn mock_challenge_from_signature(signature: &str) -> Option<&str> {
    signature
        .trim()
        .strip_prefix("paid:")
        .map(str::trim)
        .filter(|challenge_id| !challenge_id.is_empty())
}

fn x402_runtime_is_production() -> bool {
    ["NC_ENV", "APP_ENV", "RUST_ENV"].iter().any(|key| {
        env::var(key)
            .ok()
            .map(|value| value.trim().eq_ignore_ascii_case("production"))
            .unwrap_or(false)
    })
}
