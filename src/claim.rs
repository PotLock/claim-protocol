use crate::*;

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClaimType {
    Near,
    FungibleToken {
        contract_id: AccountId,
    },
    NonFungibleToken {
        contract_id: AccountId,
        token_id: String,
    },
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Claim {
    pub claim_type: ClaimType,
    pub amount: NearToken, // For NEAR and FTs. Ignored for NFTs
    pub tipper: AccountId,
    pub timestamp: u64,
    pub expires_at: u64,
    pub claimed: bool,
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClaimExternal {
    pub id: ClaimId,
    pub claim_type: String,
    pub amount: U128, // For NEAR and FTs. Ignored for NFTs
    pub tipper: AccountId,
    pub timestamp: u64,
    pub expires_at: u64,
    pub claimed: bool,
}

pub(crate) fn format_claim(claim_id: &ClaimId, claim: &Claim) -> ClaimExternal {
    let claim_type = match &claim.claim_type {
        ClaimType::Near => "Near".to_string(),
        ClaimType::FungibleToken { contract_id } => format!("FT({})", contract_id),
        ClaimType::NonFungibleToken {
            contract_id,
            token_id,
        } => format!("NFT({}, {})", contract_id, token_id),
    };
    ClaimExternal {
        id: claim_id.clone(),
        claim_type,
        amount: claim.amount.as_yoctonear().into(),
        tipper: claim.tipper.clone(),
        timestamp: claim.timestamp,
        expires_at: claim.expires_at,
        claimed: claim.claimed,
    }
}

impl Claim {
    pub fn new_near(tipper: AccountId, amount: u128) -> Self {
        Self {
            claim_type: ClaimType::Near,
            amount: NearToken::from_yoctonear(amount),
            tipper,
            timestamp: env::block_timestamp(),
            expires_at: env::block_timestamp() + crate::CLAIM_EXPIRATION_PERIOD,
            claimed: false,
        }
    }

    pub fn new_ft(tipper: AccountId, contract_id: AccountId, amount: u128) -> Self {
        Self {
            claim_type: ClaimType::FungibleToken { contract_id },
            amount: NearToken::from_yoctonear(amount),
            tipper,
            timestamp: env::block_timestamp(),
            expires_at: env::block_timestamp() + crate::CLAIM_EXPIRATION_PERIOD,
            claimed: false,
        }
    }

    pub fn new_nft(tipper: AccountId, contract_id: AccountId, token_id: String) -> Self {
        Self {
            claim_type: ClaimType::NonFungibleToken {
                contract_id,
                token_id,
            },
            amount: NearToken::from_yoctonear(0), // Not used for NFTs
            tipper,
            timestamp: env::block_timestamp(),
            expires_at: env::block_timestamp() + crate::CLAIM_EXPIRATION_PERIOD,
            claimed: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        env::block_timestamp() >= self.expires_at
    }

    pub fn amount(&self) -> u128 {
        self.amount.as_yoctonear()
    }

    pub fn tipper(&self) -> &AccountId {
        &self.tipper
    }

    pub fn token_type(&self) -> &str {
        match &self.claim_type {
            ClaimType::Near => "NEAR",
            ClaimType::FungibleToken { .. } => "FT",
            ClaimType::NonFungibleToken { .. } => "NFT",
        }
    }
}
