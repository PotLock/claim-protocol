// test vibe generated using LLM, not tested yet!

use chrono::Utc;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::NearToken;
use near_workspaces::{Account, AccountId, Contract, DevNetwork};

#[tokio::test]
async fn test_claim_protocol() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create accounts
    let alice = create_subaccount(&root, "alice").await?;
    let bob = create_subaccount(&root, "bob").await?;

    // Deploy mock contracts
    let reclaim_contract = deploy_reclaim_mock(&worker).await?;
    let ft_contract = deploy_ft_mock(&worker).await?;
    let nft_contract = deploy_nft_mock(&worker).await?;

    // Deploy and initialize claim protocol contract
    let contract = deploy_claim_protocol(&worker, reclaim_contract.id(), bob.id()).await?;

    // Register FT and NFT tokens with the claim protocol
    contract
        .call("register_token")
        .args_json(json!({
            "token_id": ft_contract.id(),
            "token_info": {"standard": "NEP141", "decimals": 18, "symbol": "MOCK", "chain": "near"}
        }))
        .transact()
        .await?
        .into_result()?;

    contract
        .call("register_token")
        .args_json(json!({
            "token_id": nft_contract.id(),
            "token_info": {"standard": "NEP171", "decimals": 0, "symbol": "NFT", "chain": "near"}
        }))
        .transact()
        .await?
        .into_result()?;

    // **Test 1: Linking an Account**
    let valid_proof = create_mock_proof("alice123", "Twitter");
    alice
        .call(contract.id(), "link_account")
        .args_json(json!({
            "platform": "Twitter",
            "handle": "alice123",
            "proof": valid_proof
        }))
        .deposit(NearToken::from_yoctonear(1)) // Adding deposit for storage
        .transact()
        .await?
        .into_result()?;

    let is_linked = contract
        .view("is_linked")
        .args_json(json!({
            "platform": "Twitter",
            "handle": "alice123"
        }))
        .await?
        .json::<bool>()?;
    assert!(is_linked, "Account should be linked");

    // **Test 2: Tipping with NEAR to a Linked Handle**
    let tip_amount = NearToken::from_near(1);
    let initial_alice_balance = alice.view_account().await?.balance;
    bob.call(contract.id(), "tip_near")
        .args_json(json!({
            "platform": "Twitter",
            "handle": "alice123"
        }))
        .deposit(tip_amount)
        .transact()
        .await?
        .into_result()?;

    let final_alice_balance = alice.view_account().await?.balance;
    assert!(
        final_alice_balance > initial_alice_balance,
        "Alice should receive the NEAR tip"
    );

    // **Test 3: Tipping with NEAR to an Unlinked Handle**
    let unlinked_handle = "bob456";
    bob.call(contract.id(), "tip_near")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unlinked_handle
        }))
        .deposit(tip_amount)
        .transact()
        .await?
        .into_result()?;

    let pending_claims_count = contract
        .view("get_pending_claims_count")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unlinked_handle
        }))
        .await?
        .json::<u64>()?;
    assert!(pending_claims_count > 0, "Pending claim should exist");

    // **Test 4: Claiming a Tip After Linking**
    let bob_proof = create_mock_proof(unlinked_handle, "Twitter");
    bob.call(contract.id(), "link_account")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unlinked_handle,
            "proof": bob_proof
        }))
        .deposit(NearToken::from_yoctonear(1)) // Adding deposit for storage
        .transact()
        .await?
        .into_result()?;

    let initial_bob_balance = bob.view_account().await?.balance;
    bob.call(contract.id(), "claim")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unlinked_handle
        }))
        .transact()
        .await?
        .into_result()?;

    let final_bob_balance = bob.view_account().await?.balance;
    assert!(
        final_bob_balance > initial_bob_balance,
        "Bob should receive the claimed NEAR"
    );

    let pending_after_claim = contract
        .view("get_pending_claims_count")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unlinked_handle
        }))
        .await?
        .json::<u64>()?;
    assert_eq!(pending_after_claim, 0, "Pending claims should be cleared");

    // **Test 5: Tipping with FT to a Linked Handle**
    // First, initialize the FT mock with some tokens for Bob
    ft_contract
        .call("initialize")
        .args_json(json!({
            "owner_id": bob.id(),
            "total_supply": U128(10000)
        }))
        .transact()
        .await?
        .into_result()?;

    // Send FT tokens via ft_transfer_call to the contract
    bob.call(ft_contract.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": contract.id(),
            "amount": U128(1000),
            "memo": null,
            "msg": json!({
                "platform": "Twitter",
                "handle": "alice123"
            }).to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Check Alice's FT balance
    let alice_ft_balance = ft_contract
        .view("ft_balance_of")
        .args_json(json!({"account_id": alice.id()}))
        .await?
        .json::<U128>()?;
    assert_eq!(alice_ft_balance.0, 1000, "Alice should receive FT tip");

    // **Test 6: Simulating an Expired Tip and Reclaiming it**
    // Create an unclaimed tip
    let unclaimed_handle = "unclaimed789";
    bob.call(contract.id(), "tip_near")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unclaimed_handle
        }))
        .deposit(tip_amount)
        .transact()
        .await?
        .into_result()?;

    // Fast forward time (this might need adjustment based on how the mock works)
    for _ in 0..100 {
        worker.fast_forward(10000).await?; // Advance many blocks
    }

    // Now try to reclaim the tip
    let initial_bob_balance = bob.view_account().await?.balance;

    // Get the claim index first
    let claims = contract
        .view("get_pending_claims")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unclaimed_handle,
            "from_index": 0,
            "limit": 10
        }))
        .await?
        .json::<Vec<serde_json::Value>>()?;

    // Reclaim the first claim (index 0)
    bob.call(contract.id(), "reclaim_tip")
        .args_json(json!({
            "platform": "Twitter",
            "handle": unclaimed_handle,
            "claim_index": 0
        }))
        .transact()
        .await?
        .into_result()?;

    let final_bob_balance = bob.view_account().await?.balance;
    assert!(
        final_bob_balance > initial_bob_balance,
        "Bob should reclaim the expired tip"
    );

    Ok(())
}

// Helper to create a mock proof structure
fn create_mock_proof(handle: &str, platform: &str) -> serde_json::Value {
    json!({
        "claim_info": {
            "provider": platform,
            "parameters": handle,
            "context": "test"
        },
        "signed_claim": {
            "claim": {
                "identifier": format!("test-identifier-{}", handle),
                "owner": "test-owner",
                "epoch": 1,
                "timestampS": Utc::now().timestamp() as u64
            },
            "signatures": ["test-signature"]
        }
    })
}

// Helper Functions
async fn create_subaccount(
    root: &Account,
    name: &str,
) -> Result<Account, Box<dyn std::error::Error>> {
    let subaccount = root
        .create_subaccount(name)
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;
    Ok(subaccount)
}

async fn deploy_reclaim_mock(
    worker: &near_workspaces::Worker<impl DevNetwork>,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let wasm = near_workspaces::compile_project("./reclaim_mock").await?;
    let contract = worker.dev_deploy(&wasm).await?;

    // Initialize the mock reclaim contract if needed
    contract
        .call("new")
        .args_json(json!({}))
        .transact()
        .await?
        .into_result()?;

    Ok(contract)
}

async fn deploy_ft_mock(
    worker: &near_workspaces::Worker<impl DevNetwork>,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let wasm = near_workspaces::compile_project("./ft_mock").await?;
    let contract = worker.dev_deploy(&wasm).await?;
    Ok(contract)
}

async fn deploy_nft_mock(
    worker: &near_workspaces::Worker<impl DevNetwork>,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let wasm = near_workspaces::compile_project("./nft_mock").await?;
    let contract = worker.dev_deploy(&wasm).await?;

    // Initialize the mock NFT contract if needed
    contract
        .call("new")
        .args_json(json!({}))
        .transact()
        .await?
        .into_result()?;

    Ok(contract)
}

async fn deploy_claim_protocol(
    worker: &near_workspaces::Worker<impl DevNetwork>,
    reclaim_id: &AccountId,
    owner_id: &AccountId,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let wasm = near_workspaces::compile_project("./claim-protocol").await?;
    let contract = worker.dev_deploy(&wasm).await?;
    contract
        .call("new")
        .args_json(json!({
            "owner_id": owner_id,
            "reclaim_contract_id": reclaim_id
        }))
        .transact()
        .await?
        .into_result()?;
    Ok(contract)
}
