use crate::*;
use near_sdk::env;
use near_sdk::serde_json::json;

pub const EVENT_JSON_PREFIX: &str = "EVENT_JSON:";

pub fn log_account_linked_event(platform: &str, handle: &str, account_id: &AccountId) {
    env::log_str(
        format!(
            "{}{}",
            EVENT_JSON_PREFIX,
            json!({
                "standard": "claim_protocol",
                "version": "1.0.0",
                "event": "account_linked",
                "data": [
                    {
                        "platform": platform,
                        "handle": handle,
                        "account_id": account_id,
                    }
                ]
            })
        )
        .as_ref(),
    );
}

pub fn log_tip_transferred_event(
    platform: &str,
    handle: &str,
    amount: u128,
    token_type: &str,
    recipient: &AccountId,
) {
    env::log_str(
        format!(
            "{}{}",
            EVENT_JSON_PREFIX,
            json!({
                "standard": "claim_protocol",
                "version": "1.0.0",
                "event": "tip_transferred",
                "data": [
                    {
                        "platform": platform,
                        "handle": handle,
                        "amount": amount.to_string(),
                        "token_type": token_type,
                        "recipient": recipient,
                    }
                ]
            })
        )
        .as_ref(),
    );
}

pub fn log_claim_created_event(
    platform: &str,
    handle: &str,
    amount: u128,
    token_type: &str,
    tipper: &AccountId,
) {
    env::log_str(
        format!(
            "{}{}",
            EVENT_JSON_PREFIX,
            json!({
                "standard": "claim_protocol",
                "version": "1.0.0",
                "event": "claim_created",
                "data": [
                    {
                        "platform": platform,
                        "handle": handle,
                        "amount": amount.to_string(),
                        "token_type": token_type,
                        "tipper": tipper,
                    }
                ]
            })
        )
        .as_ref(),
    );
}

pub fn log_claim_processed_event(
    platform: &str,
    handle: &str,
    amount: u128,
    token_type: &str,
    claimer: &AccountId,
) {
    env::log_str(
        format!(
            "{}{}",
            EVENT_JSON_PREFIX,
            json!({
                "standard": "claim_protocol",
                "version": "1.0.0",
                "event": "claim_processed",
                "data": [
                    {
                        "platform": platform,
                        "handle": handle,
                        "amount": amount.to_string(),
                        "token_type": token_type,
                        "claimer": claimer,
                    }
                ]
            })
        )
        .as_ref(),
    );
}

pub fn log_tip_reclaimed_event(
    platform: &str,
    handle: &str,
    amount: u128,
    token_type: &str,
    tipper: &AccountId,
) {
    env::log_str(
        format!(
            "{}{}",
            EVENT_JSON_PREFIX,
            json!({
                "standard": "claim_protocol",
                "version": "1.0.0",
                "event": "tip_reclaimed",
                "data": [
                    {
                        "platform": platform,
                        "handle": handle,
                        "amount": amount.to_string(),
                        "token_type": token_type,
                        "tipper": tipper,
                    }
                ]
            })
        )
        .as_ref(),
    );
}
// use near_sdk::{AccountId, log};
// use near_sdk::serde_json::json;

// pub enum Event<'a> {
//     AccountLinked {
//         platform: &'a str,
//         handle: &'a str,
//         account_id: &'a AccountId,
//     },
//     TipTransferred {
//         platform: &'a str,
//         handle: &'a str,
//         amount: u128,
//         token_type: &'a str,
//         recipient: &'a AccountId,
//     },
//     ClaimCreated {
//         platform: &'a str,
//         handle: &'a str,
//         amount: u128,
//         token_type: &'a str,
//         tipper: &'a AccountId,
//     },
//     ClaimProcessed {
//         platform: &'a str,
//         handle: &'a str,
//         amount: u128,
//         token_type: &'a str,
//         claimer: &'a AccountId,
//     },
//     TipReclaimed {
//         platform: &'a str,
//         handle: &'a str,
//         amount: u128,
//         token_type: &'a str,
//         tipper: &'a AccountId,
//     },
// }

// impl Event<'_> {
//     pub fn emit(&self) {
//         let log_message = match self {
//             Event::AccountLinked { platform, handle, account_id } => {
//                 json!({
//                     "standard": "claim_protocol",
//                     "version": "1.0.0",
//                     "event": "account_linked",
//                     "data": {
//                         "platform": platform,
//                         "handle": handle,
//                         "account_id": account_id,
//                     }
//                 })
//             },
//             Event::TipTransferred { platform, handle, amount, token_type, recipient } => {
//                 json!({
//                     "standard": "claim_protocol",
//                     "version": "1.0.0",
//                     "event": "tip_transferred",
//                     "data": {
//                         "platform": platform,
//                         "handle": handle,
//                         "amount": amount.to_string(),
//                         "token_type": token_type,
//                         "recipient": recipient,
//                     }
//                 })
//             },
//             Event::ClaimCreated { platform, handle, amount, token_type, tipper } => {
//                 json!({
//                     "standard": "claim_protocol",
//                     "version": "1.0.0",
//                     "event": "claim_created",
//                     "data": {
//                         "platform": platform,
//                         "handle": handle,
//                         "amount": amount.to_string(),
//                         "token_type": token_type,
//                         "tipper": tipper,
//                     }
//                 })
//             },
//             Event::ClaimProcessed { platform, handle, amount, token_type, claimer } => {
//                 json!({
//                     "standard": "claim_protocol",
//                     "version": "1.0.0",
//                     "event": "claim_processed",
//                     "data": {
//                         "platform": platform,
//                         "handle": handle,
//                         "amount": amount.to_string(),
//                         "token_type": token_type,
//                         "claimer": claimer,
//                     }
//                 })
//             },
//             Event::TipReclaimed { platform, handle, amount, token_type, tipper } => {
//                 json!({
//                     "standard": "claim_protocol",
//                     "version": "1.0.0",
//                     "event": "tip_reclaimed",
//                     "data": {
//                         "platform": platform,
//                         "handle": handle,
//                         "amount": amount.to_string(),
//                         "token_type": token_type,
//                         "tipper": tipper,
//                     }
//                 })
//             },
//         };

//         log!("EVENT_JSON:{}", log_message);
//     }
// }
