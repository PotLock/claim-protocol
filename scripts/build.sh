#!/bin/sh

    echo ">> Building Pot contract"
near account delete-account claim-protocol.testnet beneficiary chefcurry.testnet network-config testnet sign-with-legacy-keychain send
cargo near create-dev-account use-specific-account-id claim-protocol.testnet autogenerate-new-keypair save-to-legacy-keychain network-config testnet create
cargo near deploy --no-docker claim-protocol.testnet with-init-call new json-args '{"owner_id": "basorun.testnet", "reclaim_contract_id": "reclaim-protocol.testnet"}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-legacy-keychain send
near contract call-function as-transaction claim-protocol.testnet tip_near json-args '{"platform": "twitter", "handle": "Iam__Prometheus"}' prepaid-gas '100.0 Tgas' attached-deposit '1 NEAR' sign-as chefcurry.testnet network-config testnet sign-with-legacy-keychain send
near contract call-function as-transaction claim-protocol.testnet tip_near json-args '{"platform": "twitter", "handle": "Iam__Prometheus"}' prepaid-gas '100.0 Tgas' attached-deposit '1.25 NEAR' sign-as potlock.testnet network-config testnet sign-with-legacy-keychain send
