use crate::*;

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub enum TokenStandard {
    NEAR,
    NEP141, // Fungible Token
    NEP171, // Non-Fungible Token
}

#[near(serializers=[borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub standard: TokenStandard,
    pub decimals: u8,
    pub symbol: String,
    pub chain: String, // "near" or "solana", is this needed tho?
}

// note on cross chain tipping, settlement will be done on near, which means that tipper can tip from solana, btc, eth, base, etc.
// but recipient will get equivalent amount of NEAR tokens. or stable, if the tipper specifies?
