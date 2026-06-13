use std::collections::HashMap;
use serde::Deserialize;

/// Top-level fixture produced by harvest_prestate.py.
#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub tx_hash: String,
    pub transaction: TxData,
    pub block_header: BlockHeader,
    pub receipt: Receipt,
    pub prestate: HashMap<String, AccountState>,
}

#[derive(Debug, Deserialize)]
pub struct TxData {
    pub from: String,
    pub to: Option<String>,
    pub input: String,
    pub nonce: String,
    pub value: String,
    pub gas: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
    #[serde(rename = "maxFeePerGas")]
    pub max_fee_per_gas: Option<String>,
    #[serde(rename = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: Option<String>,
    #[serde(rename = "accessList")]
    pub access_list: Vec<AccessListItem>,
    #[serde(rename = "type")]
    pub tx_type: String,
}

#[derive(Debug, Deserialize)]
pub struct AccessListItem {
    pub address: String,
    #[serde(rename = "storageKeys")]
    pub storage_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockHeader {
    pub number: String,
    pub timestamp: String,
    pub coinbase: String,
    pub gas_limit: String,
    pub base_fee_per_gas: String,
    pub prev_randao: String,
    pub excess_blob_gas: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Receipt {
    pub gas_used: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountState {
    pub balance: String,
    pub nonce: Option<serde_json::Value>,
    pub code: Option<String>,
    pub storage: Option<HashMap<String, String>>,
}

// ── hex parsing helpers ──────────────────────────────────────────────────────

pub fn parse_u64_hex(s: &str) -> anyhow::Result<u64> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    Ok(u64::from_str_radix(s, 16)?)
}

pub fn parse_u128_hex(s: &str) -> anyhow::Result<u128> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    Ok(u128::from_str_radix(s, 16)?)
}

pub fn parse_b256_hex(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)?;
    let mut arr = [0u8; 32];
    let offset = 32usize.saturating_sub(bytes.len());
    arr[offset..].copy_from_slice(&bytes[..bytes.len().min(32)]);
    Ok(arr)
}

pub fn parse_address_hex(s: &str) -> anyhow::Result<[u8; 20]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)?;
    if bytes.len() != 20 {
        anyhow::bail!("address must be 20 bytes, got {}", bytes.len());
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub fn parse_bytes_hex(s: &str) -> anyhow::Result<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    Ok(hex::decode(s)?)
}
