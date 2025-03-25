use near_sdk::{ext_contract, AccountId, Promise};

#[ext_contract(ext_reclaim)]
pub trait ReclaimProtocol {
    #[handle_result]
    fn verify_proof(proof: crate::proof::ReclaimProof) -> Result<(), &'static str>;
}

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(receiver_id: AccountId, amount: String, memo: Option<String>);

    fn ft_transfer_call(
        receiver_id: AccountId,
        amount: String,
        memo: Option<String>,
        msg: String,
    ) -> Promise;
}

#[ext_contract(ext_nft)]
pub trait NonFungibleToken {
    fn nft_transfer(
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
    );

    fn nft_transfer_call(
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> Promise;
}
