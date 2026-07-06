use uuid::Uuid;

pub trait IChallengeSupervisor: Send + Sync {
    fn get_challenge_text(&self) -> &str;

    fn is_challenge_ok(&self, text: &str) -> bool;
}

pub struct ChallengeSupervisor {
    challenge_text: String,
}

impl ChallengeSupervisor {
    pub fn new() -> Self {
        Self {
            challenge_text: Uuid::new_v4().to_string(),
        }
    }

    pub fn with_challenge(text: impl Into<String>) -> Self {
        Self {
            challenge_text: text.into(),
        }
    }
}

impl Default for ChallengeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl IChallengeSupervisor for ChallengeSupervisor {
    fn get_challenge_text(&self) -> &str {
        &self.challenge_text
    }

    fn is_challenge_ok(&self, text: &str) -> bool {
        self.challenge_text == text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_itself() {
        let cs = ChallengeSupervisor::new();
        let text = cs.get_challenge_text().to_owned();
        assert!(cs.is_challenge_ok(&text));
    }

    #[test]
    fn challenge_rejects_wrong_text() {
        let cs = ChallengeSupervisor::new();
        assert!(!cs.is_challenge_ok("wrong"));
    }

    #[test]
    fn predetermined_challenge() {
        let cs = ChallengeSupervisor::with_challenge("test-challenge");
        assert_eq!(cs.get_challenge_text(), "test-challenge");
        assert!(cs.is_challenge_ok("test-challenge"));
        assert!(!cs.is_challenge_ok("other"));
    }

    #[test]
    fn two_supervisors_have_different_challenges() {
        let a = ChallengeSupervisor::new();
        let b = ChallengeSupervisor::new();
        assert_ne!(a.get_challenge_text(), b.get_challenge_text());
    }
}
