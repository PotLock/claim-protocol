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
use claim::{format_claim, Claim, ClaimExternal, ClaimType};
use proof::ReclaimProof;
use token::TokenInfo;

type ClaimId = u64;

/// Gas for FT transfers
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas for NFT transfers
const GAS_FOR_NFT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas for Reclaim Protocol verification
const GAS_FOR_RECLAIM_VERIFY: Gas = Gas::from_tgas(27);
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
    HandleClaims,
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
        format!("{}:{}", self.platform, self.handle)
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
    pub linked_accounts: IterableMap<String, AccountId>,

    pub next_claim_id: ClaimId,

    /// Mapping of social media handles to claim IDs
    pub claims_by_id: IterableMap<ClaimId, Claim>,
    /// Claims for unlinked accounts
    pub handle_claims: IterableMap<String, IterableSet<ClaimId>>,

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
            handle_claims: IterableMap::new(StorageKey::HandleClaims),
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
            !self
                .linked_accounts
                .contains_key(&social_handle.to_string()),
            "Handle already linked"
        );

        match serde_json::from_str::<serde_json::Value>(&proof.claimInfo.context) {
            Ok(context_json) => {
                if let Some(extracted_param) = context_json.get("extractedParameters") {
                    if let Some(screen_name_value) = extracted_param.get("screen_name") {
                        if let Some(screen_name) = screen_name_value.as_str() {
                            env::log_str(&format!("screen_name: {}, {}", screen_name, handle));
                            assert_eq!(
                                screen_name, handle,
                                "Proven handle does not match passed handle"
                            );
                        }
                    } else {
                        env::panic_str("screen_name not found in extractedParameters");
                    }
                } else {
                    env::panic_str("needed params not found");
                }
            }
            Err(_) => env::panic_str("Failed to parse context Json for verification"),
        }

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
                .insert(social_handle.to_string(), account_id.clone());
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

        if let Some(recipient) = self.linked_accounts.get(&social_handle.to_string()) {
            // Direct transfer for linked accounts
            log_tip_transferred_event(
                &social_handle.platform,
                &social_handle.handle,
                amount.as_yoctonear().into(),
                "NEAR",
                &recipient,
            );

            PromiseOrValue::Promise(Promise::new(recipient.clone()).transfer(amount))
        } else {
            // Store as pending claim
            let claim = Claim::new_near(
                env::predecessor_account_id(),
                amount.as_yoctonear(),
                social_handle.to_string(),
            );

            self.store_claim(social_handle, claim);
            PromiseOrValue::Value(())
        }
    }

    // Internal helper to store claims
    fn store_claim(&mut self, social_handle: SocialHandle, claim: Claim) {
        let claim_id = self.next_claim_id;
        self.next_claim_id += 1;
        self.claims_by_id.insert(claim_id, claim.clone());
        let storage_key = StorageKey::ClaimsByHandle {
            platform: social_handle.platform.clone(),
            handle: social_handle.handle.clone(),
        };

        let empty_handle_claim: IterableSet<ClaimId> = IterableSet::new(storage_key);

        let claim_ids = self
            .handle_claims
            .entry(social_handle.to_string())
            .or_insert(empty_handle_claim);

        claim_ids.insert(claim_id);

        log_claim_created_event(
            &social_handle.platform,
            &social_handle.handle,
            claim.amount().into(),
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
            .get(&social_handle.to_string())
            .unwrap_or_else(|| env::panic_str("Account must be linked before claiming."));
        require!(
            env::predecessor_account_id().eq(account_id),
            "Only the linked account can claim tips"
        );

        // Process claims if any exist
        if let Some(claims_ids) = self.handle_claims.get_mut(&social_handle.to_string()) {
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

            for claim_id in claims_ids.iter().take(MAX_CLAIMS_PER_BATCH).cloned() {
                if let Some(claim) = self.claims_by_id.get(&claim_id) {
                    if claim.is_expired() {
                        // Track expired claims for removal
                        // claims_vector.remove(claim);
                        continue;
                    }
                    match &claim.claim_type {
                        ClaimType::Near => {
                            // NEAR transfer
                            Promise::new(account_id.clone())
                                .transfer(claim.amount)
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
            // TODO: maybe merge this two events into one? since they emit same params?
            if reclaim_trf.is_some() {
                log_tip_reclaimed_event(
                    &social_handle.platform,
                    &social_handle.handle,
                    claim.amount().into(),
                    claim.token_type(),
                    claim.tipper(),
                );
            } else {
                claim.claimed = true;
                log_claim_processed_event(
                    &social_handle.platform,
                    &social_handle.handle,
                    claim.amount().into(),
                    &token_type,
                    &recipient,
                );
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
                    .transfer(claim.amount)
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

    pub fn set_reclaim_contract(&mut self, contract_id: AccountId) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can change Reclaim contract"
        );

        self.reclaim_contract_id = contract_id.clone();

        env::log_str(&format!("Reclaim contract changed to {}", contract_id));
    }

    /// Get the current Reclaim Protocol contract address
    pub fn get_reclaim_contract(&self) -> AccountId {
        self.reclaim_contract_id.clone()
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
        if let Some(recipient) = self.linked_accounts.get(&social_handle.to_string()) {
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
                amount_u128.into(),
                "FT",
                recipient,
            );
        } else {
            // Store as a claim for later
            let claim = Claim::new_ft(
                sender_id,
                ft_contract_id,
                amount_u128,
                social_handle.to_string(),
            );
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
        if let Some(recipient) = self.linked_accounts.get(&social_handle.to_string()) {
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
                0.into(), // NFTs don't have amount
                "NFT",
                recipient,
            );
        } else {
            // Store as a claim for later
            let claim = Claim::new_nft(
                sender_id,
                nft_contract_id,
                token_id,
                social_handle.to_string(),
            );
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
        self.linked_accounts
            .contains_key(&social_handle.to_string())
    }

    pub fn get_claim_by_id(&self, claim_id: ClaimId) -> Option<Claim> {
        self.claims_by_id.get(&claim_id).cloned()
    }

    /// Get the linked account for a social handle
    pub fn get_linked_account(&self, platform: String, handle: String) -> Option<AccountId> {
        let social_handle = SocialHandle::new(platform, handle);
        self.linked_accounts
            .get(&social_handle.to_string())
            .cloned()
    }

    /// Get the count of pending claims for a social handle
    pub fn get_pending_claims_count(&self, platform: String, handle: String) -> u64 {
        let social_handle = SocialHandle::new(platform, handle);
        if let Some(claims) = self.handle_claims.get(&social_handle.to_string()) {
            claims
                .iter()
                .filter(|claim_id| {
                    if let Some(claim) = self.claims_by_id.get(claim_id) {
                        !claim.claimed && !claim.is_expired()
                    } else {
                        false
                    }
                })
                .count() as u64
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
    ) -> Vec<ClaimExternal> {
        let social_handle = SocialHandle::new(platform, handle);

        if let Some(claim_ids) = self.handle_claims.get(&social_handle.to_string()) {
            // First collect all claim IDs into a Vec
            let claim_id_vec: Vec<ClaimId> = claim_ids.iter().cloned().collect();

            // Apply pagination
            let start = from_index as usize;
            let end = std::cmp::min(start + limit as usize, claim_id_vec.len());

            if start < end {
                // Map each claim ID to its corresponding claim
                claim_id_vec[start..end]
                    .iter()
                    .filter_map(|claim_id| {
                        let claim = self.claims_by_id.get(claim_id)?;
                        if !claim.claimed && !claim.is_expired() {
                            Some(format_claim(claim_id, claim))
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    pub fn get_linked_handles(&self, from_index: u64, limit: u64) -> Vec<(String, AccountId)> {
        let keys = self.linked_accounts.keys().collect::<Vec<_>>();
        let values = self.linked_accounts.values().collect::<Vec<_>>();

        let start = from_index as usize;
        let end = std::cmp::min(start + limit as usize, keys.len());

        if start < end {
            (start..end)
                .map(|i| (keys[i].clone(), values[i].clone()))
                .collect()
        } else {
            vec![]
        }
    }

    /// Get all social handles linked to a specific account
    pub fn get_handles_by_account(&self, account_id: AccountId) -> Vec<String> {
        self.linked_accounts
            .iter()
            .filter(|(_, linked_account)| **linked_account == account_id)
            .map(|(handle, _)| handle.clone())
            .collect()
    }

    // pub fn get_claims_by_tipper(
    //     &self,
    //     tipper: AccountId,
    //     from_index: u64,
    //     limit: u64,
    // ) -> Vec<Claim> {
    //     let claims: Vec<Claim> = self
    //         .claims_by_id
    //         .iter()
    //         .filter_map(|(_, claim)| {
    //             if claim.tipper() == &tipper {
    //                 Some(claim.clone())
    //             } else {
    //                 None
    //             }
    //         })
    //         .collect();

    //     let start = from_index as usize;
    //     let end = std::cmp::min(start + limit as usize, claims.len());

    //     if start < end {
    //         claims[start..end].to_vec()
    //     } else {
    //         vec![]
    //     }
    // }

    // inefficient
    pub fn get_claims_by_tipper(
        &self,
        tipper: AccountId,
        from_index: u64,
        limit: u64,
    ) -> Vec<ClaimExternal> {
        let claims: Vec<ClaimExternal> = self
            .claims_by_id
            .iter()
            .filter_map(|(claim_id, claim)| {
                if claim.tipper() == &tipper {
                    Some(format_claim(claim_id, claim))
                } else {
                    None
                }
            })
            .collect();

        let start = from_index as usize;
        let end = std::cmp::min(start + limit as usize, claims.len());

        if start < end {
            claims[start..end].to_vec()
        } else {
            vec![]
        }
    }

    pub fn get_all_claims_for_handle(
        &self,
        platform: String,
        handle: String,
        from_index: u64,
        limit: u64,
    ) -> Vec<ClaimExternal> {
        let social_handle = SocialHandle::new(platform, handle);

        if let Some(claim_ids) = self.handle_claims.get(&social_handle.to_string()) {
            // First collect all claim IDs into a Vec
            let claim_id_vec: Vec<ClaimId> = claim_ids.iter().cloned().collect();

            // Apply pagination
            let start = from_index as usize;
            let end = std::cmp::min(start + limit as usize, claim_id_vec.len());

            if start < end {
                // Map each claim ID to its corresponding claim
                claim_id_vec[start..end]
                    .iter()
                    .map(|claim_id| {
                        let claim = self.claims_by_id.get(claim_id).unwrap(); // Unwrap is safe here because we know the claim_id exists
                        format_claim(claim_id, claim)
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }
}
