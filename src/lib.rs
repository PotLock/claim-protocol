use near_sdk::json_types::U128;
use near_sdk::store::{IterableMap, IterableSet};
use near_sdk::{
    env, near, near_bindgen, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError, PromiseOrValue,
};

mod claim;
mod events;
mod external;
mod proof;
mod token;
// mod utils;

use crate::events::*;
use claim::{Claim, ClaimType};
use proof::ReclaimProof;
use token::TokenInfo;

type ClaimId = u64;

/// Gas for FT transfers
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas for NFT transfers
const GAS_FOR_NFT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas for Reclaim Protocol verification
const GAS_FOR_RECLAIM_VERIFY: Gas = Gas::from_tgas(20);
/// Maximum time allowed between proof generation and submission (5 minutes)
const MAX_PROOF_AGE: u64 = 5 * 60 * 1_000_000_000;
/// Claim expiration period (90 days)
const CLAIM_EXPIRATION_PERIOD: u64 = 90 * 24 * 60 * 60 * 1_000_000_000;
/// Maximum claims to process in a single batch
const MAX_CLAIMS_PER_BATCH: usize = 100;

/// Gas for cross-contract calls
const XCC_GAS_DEFAULT: u64 = 10;

pub const TOKEN_REGISTRATION_FEE: NearToken = NearToken::from_near(1);

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
pub enum StorageKey {
    LinkedAccounts,
    PendingClaims,
    ClaimsByHandle { platform: String, handle: String },
    ClaimsById,
    SupportedTokens,
}

/// Platform and handle combined key
#[near(serializers=[borsh, json])]
#[derive(Eq, Ord, Hash, Clone, Debug, PartialEq, PartialOrd)]
pub struct SocialHandle {
    pub platform: String,
    pub handle: String,
}

impl SocialHandle {
    pub fn new(platform: String, handle: String) -> Self {
        Self {
            platform: platform.to_lowercase(),
            handle: handle.to_lowercase(),
        }
    }

    pub fn to_string(&self) -> String {
        format!("{:?}:{:?}", self.platform, self.handle)
    }
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    /// Owner account ID
    pub owner_id: AccountId,

    /// Reclaim Protocol contract ID for verification
    pub reclaim_contract_id: AccountId,

    /// Mapping of social media handles to NEAR accounts
    pub linked_accounts: IterableMap<SocialHandle, AccountId>,

    pub next_claim_id: ClaimId,

    /// Mapping of social media handles to claim IDs
    pub claims_by_id: IterableMap<ClaimId, Claim>,
    /// Pending claims for unlinked accounts
    pub pending_claims: IterableMap<SocialHandle, IterableSet<ClaimId>>,

    /// Supported tokens (FTs and NFTs)
    pub supported_tokens: IterableMap<AccountId, TokenInfo>,

    /// Contract paused state
    pub paused: bool,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId, reclaim_contract_id: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        Self {
            owner_id,
            reclaim_contract_id,
            linked_accounts: IterableMap::new(StorageKey::LinkedAccounts),
            next_claim_id: 1,
            claims_by_id: IterableMap::new(StorageKey::ClaimsById),
            pending_claims: IterableMap::new(StorageKey::PendingClaims),
            supported_tokens: IterableMap::new(StorageKey::SupportedTokens),
            paused: false,
        }
    }

    /// Link a social media handle to a NEAR account
    #[payable]
    pub fn link_account(
        &mut self,
        platform: String,
        handle: String,
        proof: ReclaimProof,
    ) -> Promise {
        require!(!self.paused, "Contract is paused");
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "Requires attached deposit"
        );

        let social_handle = SocialHandle::new(platform.clone(), handle.clone());
        require!(
            !self.linked_accounts.contains_key(&social_handle),
            "Handle already linked"
        );

        // Verify proof through Reclaim Protocol
        external::ext_reclaim::ext(self.reclaim_contract_id.clone())
            .with_static_gas(GAS_FOR_RECLAIM_VERIFY)
            .verify_proof(proof)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(5))
                    .on_link_account_verified(social_handle, env::predecessor_account_id()),
            )
    }

    #[private]
    pub fn on_link_account_verified(
        &mut self,
        social_handle: SocialHandle,
        account_id: AccountId,
        #[callback_result] verification_result: Result<(), near_sdk::PromiseError>,
    ) {
        if verification_result.is_err() {
            env::panic_str("Proof verification failed")
        } else {
            self.linked_accounts
                .insert(social_handle.clone(), account_id.clone());
            log_account_linked_event(&social_handle.platform, &social_handle.handle, &account_id);
        }
    }

    /// Tip with native NEAR
    #[payable]
    pub fn tip_near(&mut self, platform: String, handle: String) -> PromiseOrValue<()> {
        require!(!self.paused, "Contract is paused");
        let amount = env::attached_deposit();
        require!(
            amount > NearToken::from_yoctonear(0),
            "Requires attached deposit"
        );

        let social_handle = SocialHandle::new(platform, handle);

        if let Some(recipient) = self.linked_accounts.get(&social_handle) {
            // Direct transfer for linked accounts
            log_tip_transferred_event(
                &social_handle.platform,
                &social_handle.handle,
                amount.as_yoctonear(),
                "NEAR",
                &recipient,
            );

            PromiseOrValue::Promise(Promise::new(recipient.clone()).transfer(amount))
        } else {
            // Store as pending claim
            let claim = Claim::new_near(env::predecessor_account_id(), amount.as_yoctonear());

            self.store_claim(social_handle, claim);
            PromiseOrValue::Value(())
        }
    }

    // Internal helper to store claims
    fn store_claim(&mut self, social_handle: SocialHandle, claim: Claim) {
        let claim_id = self.next_claim_id.clone();
        self.next_claim_id += 1;
        self.claims_by_id.insert(claim_id.clone(), claim.clone());
        let storage_key = StorageKey::ClaimsByHandle {
            platform: social_handle.platform.clone(),
            handle: social_handle.handle.clone(),
        };

        let mut empty_pending_claim: IterableSet<ClaimId> = IterableSet::new(storage_key);

        let claim_ids = self
            .pending_claims
            .get_mut(&social_handle)
            .unwrap_or_else(|| &mut empty_pending_claim);

        claim_ids.insert(claim_id);

        log_claim_created_event(
            &social_handle.platform,
            &social_handle.handle,
            claim.amount(),
            claim.token_type(),
            claim.tipper(),
        );
    }

    #[payable]
    pub fn claim(&mut self, platform: String, handle: String) {
        require!(!self.paused, "Contract is paused");

        let social_handle = SocialHandle::new(platform.clone(), handle.clone());

        let account_id = self
            .linked_accounts
            .get(&social_handle)
            .unwrap_or_else(|| env::panic_str("Account must be linked before claiming."));
        require!(
            env::predecessor_account_id().eq(account_id),
            "Only the linked account can claim tips"
        );

        // Process claims if any exist
        if let Some(claims_ids) = self.pending_claims.get_mut(&social_handle) {
            if claims_ids.is_empty() {
                env::panic_str("No pending claims to process.");
            }

            env::log_str(&format!(
                "Processing up to {} claims for {:?}:{:?} (total: {})",
                MAX_CLAIMS_PER_BATCH,
                social_handle.platform,
                social_handle.handle,
                claims_ids.len()
            ));

            let mut expired_claim_ids = Vec::new();

            for claim_id in claims_ids.iter().take(MAX_CLAIMS_PER_BATCH).cloned() {
                if let Some(claim) = self.claims_by_id.get(&claim_id) {
                    if claim.is_expired() {
                        // Track expired claims for removal
                        expired_claim_ids.push(claim_id);
                        // claims_vector.remove(claim);
                        continue;
                    }
                    match &claim.claim_type {
                        ClaimType::Near => {
                            // NEAR transfer
                            Promise::new(account_id.clone())
                                .transfer(NearToken::from_yoctonear(claim.amount))
                                .then(
                                    Self::ext(env::current_account_id())
                                        .with_static_gas(Gas::from_tgas(5))
                                        .on_transfer_complete(
                                            social_handle.clone(),
                                            "NEAR".to_string(),
                                            claim_id,
                                            account_id.clone(),
                                            None,
                                        ),
                                )
                        }
                        ClaimType::FungibleToken { contract_id } => {
                            // FT transfer with callback
                            external::ext_ft::ext(contract_id.clone())
                                .with_attached_deposit(NearToken::from_yoctonear(1))
                                .with_static_gas(Gas::from_tgas(XCC_GAS_DEFAULT))
                                .ft_transfer(
                                    account_id.clone(),
                                    claim.amount.to_string(),
                                    Some(format!("Claimed tip from {}", claim.tipper)),
                                )
                                .then(
                                    Self::ext(env::current_account_id())
                                        .with_static_gas(Gas::from_tgas(5))
                                        .on_transfer_complete(
                                            social_handle.clone(),
                                            "FT".to_string(),
                                            claim_id,
                                            account_id.clone(),
                                            None,
                                        ),
                                )
                        }
                        ClaimType::NonFungibleToken {
                            contract_id,
                            token_id,
                        } => {
                            // NFT transfer with callback
                            external::ext_nft::ext(contract_id.clone())
                                .with_attached_deposit(NearToken::from_yoctonear(1))
                                .with_static_gas(Gas::from_tgas(XCC_GAS_DEFAULT))
                                .nft_transfer(
                                    account_id.clone(),
                                    token_id.clone(),
                                    None,
                                    Some(format!("Claimed tip from {}", claim.tipper)),
                                )
                                .then(
                                    Self::ext(env::current_account_id())
                                        .with_static_gas(Gas::from_tgas(5))
                                        .on_transfer_complete(
                                            social_handle.clone(),
                                            "NFT".to_string(),
                                            claim_id,
                                            account_id.clone(),
                                            None,
                                        ),
                                )
                        }
                    };
                }
            }
            if !expired_claim_ids.is_empty() {
                for claim_id in expired_claim_ids {
                    claims_ids.remove(&claim_id);
                }
            }
        } else {
            env::log_str("No pending claims found for this handle");
        }
    }

    #[private]
    pub fn on_transfer_complete(
        &mut self,
        social_handle: SocialHandle,
        token_type: String,
        claim_id: ClaimId,
        recipient: AccountId,
        reclaim_trf: Option<bool>,
        #[callback_result] transfer_result: Result<(), PromiseError>,
    ) {
        if transfer_result.is_err() {
            env::log_str(&format!(
                "Transfer failed for {} {} for {:?}:{:?}",
                if reclaim_trf.is_some() {
                    "reclaim of"
                } else {
                    "claim of"
                },
                token_type,
                social_handle.platform,
                social_handle.handle
            ));
        } else if let Some(claim) = self.claims_by_id.get_mut(&claim_id) {
            if let Some(claim_ids) = self.pending_claims.get_mut(&social_handle) {
                claim_ids.remove(&claim_id);
                if claim_ids.is_empty() {
                    self.pending_claims.remove(&social_handle);
                    env::log_str(&format!(
                        "All claims processed for {:?}:{:?}",
                        social_handle.platform, social_handle.handle
                    ));
                }
                // TODO: maybe merge this two events into one? since they emit same params?
                if reclaim_trf.is_some() {
                    log_tip_reclaimed_event(
                        &social_handle.platform,
                        &social_handle.handle,
                        claim.amount(),
                        claim.token_type(),
                        claim.tipper(),
                    );
                } else {
                    claim.claimed = true;
                    log_claim_processed_event(
                        &social_handle.platform,
                        &social_handle.handle,
                        claim.amount(),
                        &token_type,
                        &recipient,
                    );
                }
            }
        }
    }

    pub fn reclaim_tip(&mut self, platform: String, handle: String, claim_id: ClaimId) -> Promise {
        require!(!self.paused, "Contract is paused");

        let social_handle = SocialHandle::new(platform, handle);

        // Get the claims for this handle
        if let Some(claim) = self.claims_by_id.get_mut(&claim_id) {
            assert!(!claim.claimed, "tip has been claimed");
            assert!(claim.is_expired(), "claim is not yet expired");

            // Verify the caller is the original tipper
            require!(
                &env::predecessor_account_id() == claim.tipper(),
                "Only the original tipper can reclaim funds"
            );

            match &claim.claim_type {
                ClaimType::Near => Promise::new(env::predecessor_account_id())
                    .transfer(NearToken::from_yoctonear(claim.amount))
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(Gas::from_tgas(5))
                            .on_transfer_complete(
                                social_handle.clone(),
                                "NEAR".to_string(),
                                claim_id,
                                env::predecessor_account_id(),
                                Some(true),
                            ),
                    ),
                ClaimType::FungibleToken { contract_id } => {
                    external::ext_ft::ext(contract_id.clone())
                        .with_attached_deposit(NearToken::from_yoctonear(1))
                        .with_static_gas(GAS_FOR_FT_TRANSFER)
                        .ft_transfer(
                            env::predecessor_account_id(),
                            claim.amount.to_string(),
                            Some("Reclaimed expired tip".to_string()),
                        )
                        .then(
                            Self::ext(env::current_account_id())
                                .with_static_gas(Gas::from_tgas(5))
                                .on_transfer_complete(
                                    social_handle.clone(),
                                    "FT".to_string(),
                                    claim_id,
                                    env::predecessor_account_id(),
                                    Some(true),
                                ),
                        )
                }
                ClaimType::NonFungibleToken {
                    contract_id,
                    token_id,
                } => external::ext_nft::ext(contract_id.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(GAS_FOR_NFT_TRANSFER)
                    .nft_transfer(
                        env::predecessor_account_id(),
                        token_id.clone(),
                        None,
                        Some("Reclaimed expired tip".to_string()),
                    )
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(Gas::from_tgas(5))
                            .on_transfer_complete(
                                social_handle.clone(),
                                "NFT".to_string(),
                                claim_id,
                                env::predecessor_account_id(),
                                Some(true),
                            ),
                    ),
            }
        } else {
            env::panic_str("No claims found for this handle");
        }
    }

    pub fn pause(&mut self) {
        // Only owner can pause
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can pause the contract"
        );

        require!(!self.paused, "Contract is already paused");
        self.paused = true;

        env::log_str("Contract paused by owner");
    }

    /// Unpause the contract (owner only)
    pub fn unpause(&mut self) {
        // Only owner can unpause
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can unpause the contract"
        );

        require!(self.paused, "Contract is not paused");
        self.paused = false;

        env::log_str("Contract unpaused by owner");
    }

    /// Check if contract is paused
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn register_token(&mut self, token_id: AccountId, token_info: TokenInfo) {
        // Only owner can register tokens
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can register tokens"
        );

        self.supported_tokens.insert(token_id, token_info);
    }

    pub fn remove_token(&mut self, token_id: AccountId) {
        // Only owner can remove tokens
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can remove tokens"
        );

        self.supported_tokens.remove(&token_id);
    }

    #[payable]
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(!self.paused, "Contract is paused");

        // Parse the message to get platform and handle
        let parsed_msg: serde_json::Value =
            serde_json::from_str(&msg).unwrap_or_else(|_| env::panic_str("Invalid message format"));

        let platform = parsed_msg["platform"]
            .as_str()
            .unwrap_or_else(|| env::panic_str("Missing platform field"))
            .to_string();

        let handle = parsed_msg["handle"]
            .as_str()
            .unwrap_or_else(|| env::panic_str("Missing handle field"))
            .to_string();

        let social_handle = SocialHandle::new(platform, handle);
        let ft_contract_id = env::predecessor_account_id();

        // Verify token is supported
        require!(
            self.supported_tokens.contains_key(&ft_contract_id),
            "Unsupported token"
        );

        let amount_u128 = amount.0;

        // If the handle is linked, forward the FT to the linked account
        if let Some(recipient) = self.linked_accounts.get(&social_handle) {
            // Forward the tokens to the recipient
            external::ext_ft::ext(ft_contract_id.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(GAS_FOR_FT_TRANSFER)
                .ft_transfer(
                    recipient.clone(),
                    amount.0.to_string(),
                    Some(format!("Tip from {}", sender_id)),
                );

            log_tip_transferred_event(
                &social_handle.platform,
                &social_handle.handle,
                amount_u128,
                "FT",
                recipient,
            );
        } else {
            // Store as a claim for later
            let claim = Claim::new_ft(sender_id, ft_contract_id, amount_u128);
            self.store_claim(social_handle, claim);
        }

        // Return 0 to keep all tokens in the contract
        PromiseOrValue::Value(U128(0))
    }

    #[payable]
    pub fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        _previous_owner_id: AccountId,
        token_id: String,
        msg: String,
    ) -> PromiseOrValue<bool> {
        require!(!self.paused, "Contract is paused");

        // Parse the message to get platform and handle
        let parsed_msg: serde_json::Value =
            serde_json::from_str(&msg).unwrap_or_else(|_| env::panic_str("Invalid message format"));

        let platform = parsed_msg["platform"]
            .as_str()
            .unwrap_or_else(|| env::panic_str("Missing platform field"))
            .to_string();

        let handle = parsed_msg["handle"]
            .as_str()
            .unwrap_or_else(|| env::panic_str("Missing handle field"))
            .to_string();

        let social_handle = SocialHandle::new(platform, handle);
        let nft_contract_id = env::predecessor_account_id();

        // Verify token is supported
        require!(
            self.supported_tokens.contains_key(&nft_contract_id),
            "Unsupported token"
        );

        // If the handle is linked, forward the NFT to the linked account
        if let Some(recipient) = self.linked_accounts.get(&social_handle) {
            // Forward the NFT to the recipient
            external::ext_nft::ext(nft_contract_id.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(GAS_FOR_NFT_TRANSFER)
                .nft_transfer(
                    recipient.clone(),
                    token_id.clone(),
                    None,
                    Some(format!("Tip from {}", sender_id)),
                );

            log_tip_transferred_event(
                &social_handle.platform,
                &social_handle.handle,
                0, // NFTs don't have amount
                "NFT",
                recipient,
            );
        } else {
            // Store as a claim for later
            let claim = Claim::new_nft(sender_id, nft_contract_id, token_id);
            self.store_claim(social_handle, claim);
        }

        // Return false to keep the NFT in the contract
        PromiseOrValue::Value(false)
    }

    pub fn get_token_info(&self, token_id: AccountId) -> Option<TokenInfo> {
        self.supported_tokens.get(&token_id).cloned()
    }

    pub fn get_supported_tokens(&self, from_index: u64, limit: u64) -> Vec<(AccountId, TokenInfo)> {
        let keys = self.supported_tokens.keys().cloned().collect::<Vec<_>>();
        let values = self.supported_tokens.values().cloned().collect::<Vec<_>>();

        let start = from_index as u32;
        let end = std::cmp::min(start + limit as u32, keys.len() as u32);

        (start..end)
            .map(|i| {
                (
                    keys.get(i as usize).cloned().unwrap(),
                    values.get(i as usize).cloned().unwrap(),
                )
            })
            .collect()
    }

    pub fn is_linked(&self, platform: String, handle: String) -> bool {
        let social_handle = SocialHandle::new(platform, handle);
        self.linked_accounts.contains_key(&social_handle)
    }

    pub fn get_claim_by_id(&self, claim_id: ClaimId) -> Option<Claim> {
        self.claims_by_id.get(&claim_id).cloned()
    }

    /// Get the linked account for a social handle
    pub fn get_linked_account(&self, platform: String, handle: String) -> Option<AccountId> {
        let social_handle = SocialHandle::new(platform, handle);
        self.linked_accounts.get(&social_handle).cloned()
    }

    /// Get the count of pending claims for a social handle
    pub fn get_pending_claims_count(&self, platform: String, handle: String) -> u64 {
        let social_handle = SocialHandle::new(platform, handle);
        if let Some(claims) = self.pending_claims.get(&social_handle) {
            claims.len() as u64
        } else {
            0
        }
    }

    /// Get pending claims for a social handle
    pub fn get_pending_claims(
        &self,
        platform: String,
        handle: String,
        from_index: u64,
        limit: u64,
    ) -> Vec<Claim> {
        let social_handle = SocialHandle::new(platform, handle);

        if let Some(claims) = self.pending_claims.get(&social_handle) {
            let start = from_index as usize;
            let end = std::cmp::min(start + limit as usize, claims.len() as usize);

            if start < end {
                (start..end)
                    .map(|i| self.claims_by_id.get(&(i as u64)).unwrap().clone())
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }
}
