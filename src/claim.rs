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
    pub amount: u128, // For NEAR and FTs. Ignored for NFTs
    pub tipper: AccountId,
    pub timestamp: u64,
    pub expires_at: u64,
    pub claimed: bool,
}

impl Claim {
    pub fn new_near(tipper: AccountId, amount: u128) -> Self {
        Self {
            claim_type: ClaimType::Near,
            amount,
            tipper,
            timestamp: env::block_timestamp(),
            expires_at: env::block_timestamp() + crate::CLAIM_EXPIRATION_PERIOD,
            claimed: false,
        }
    }

    pub fn new_ft(tipper: AccountId, contract_id: AccountId, amount: u128) -> Self {
        Self {
            claim_type: ClaimType::FungibleToken { contract_id },
            amount,
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
            amount: 0, // Not used for NFTs
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
        self.amount
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
