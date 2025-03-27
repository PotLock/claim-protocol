use crate::*;

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct ClaimInfo {
    pub provider: String,   // Equivalent to platform
    pub parameters: String, // Additional parameters (could include handle)
    pub context: String,    // Additional context
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct CompleteClaimData {
    pub identifier: String,
    pub owner: String,
    pub epoch: u64,
    pub timestampS: u64,
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct SignedClaim {
    pub claim: CompleteClaimData,
    pub signatures: Vec<String>,
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct ReclaimProof {
    pub claimInfo: ClaimInfo,
    pub signedClaim: SignedClaim,
}

impl ReclaimProof {
    pub fn is_recent(&self, current_time: u64) -> bool {
        // Convert seconds to nanoseconds for consistency with NEAR block timestamp
        let proof_time_ns = self.signedClaim.claim.timestampS * 1_000_000_000;
        current_time.saturating_sub(proof_time_ns) < crate::MAX_PROOF_AGE
    }

    pub fn get_platform(&self) -> String {
        self.claimInfo.provider.clone()
    }

    // Extract handle from parameters - this would depend on the actual format
    // For simplicity, we'll assume parameters directly contains the handle
    pub fn get_handle(&self) -> String {
        self.claimInfo.parameters.clone()
    }
}
