use std::str::FromStr;
use std::time::Duration;

use boltz_client::network::{BitcoinChain, Chain as BoltzChain, LiquidChain};
use boltz_client::swaps::boltz::{
    BoltzApiClientV2, CreateChainRequest, CreateReverseRequest, CreateSubmarineRequest,
    BOLTZ_MAINNET_URL_V2, BOLTZ_REGTEST, BOLTZ_TESTNET_URL_V2,
};
use boltz_client::util::secrets::Preimage as BoltzPreimage;
use boltz_client::{bitcoin::PublicKey as BoltzPublicKey, Bolt11Invoice};
use chrono::TimeZone;
use serde::Serialize;
use thiserror::Error;

use crate::Network;

#[derive(Error, Debug)]
pub enum PaymentError {
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Network error: {0}")]
    Network(String),
}

pub struct BoltzService {
    client: BoltzApiClientV2,
    network: Network,
    boltz_api_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzSubmarineSwapCreated {
    pub id: String,
    pub flow: String,
    pub network: String,
    pub boltz_api_url: String,
    pub status: String,
    pub invoice_amount_sat: u64,
    pub expected_amount_sat: u64,
    pub lockup_address: String,
    pub timeout_block_height: u64,
    pub pair_hash: String,
    pub bip21: String,
    pub invoice_expiry_seconds: u64,
    pub invoice_expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzLightningReceiveCreated {
    pub id: String,
    pub flow: String,
    pub network: String,
    pub boltz_api_url: String,
    pub status: String,
    pub invoice_amount_sat: u64,
    pub expected_onchain_amount_sat: u64,
    pub lockup_address: String,
    pub timeout_block_height: u64,
    pub pair_hash: String,
    pub invoice: String,
    pub invoice_expiry_seconds: u64,
    pub invoice_expires_at: String,
    pub preimage_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzChainSwapCreated {
    pub id: String,
    pub flow: String,
    pub network: String,
    pub boltz_api_url: String,
    pub status: String,
    pub amount_sat: u64,
    pub expected_amount_sat: u64,
    pub lockup_address: String,
    pub claim_lockup_address: String,
    pub timeout_block_height: u64,
    pub pair_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bip21: Option<String>,
    pub preimage_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzChainSwapPairInfo {
    pub pair_hash: String,
    pub min_amount_sat: u64,
    pub max_amount_sat: u64,
    pub fee_percentage: f64,
    pub miner_fee_lockup_sat: u64,
    pub miner_fee_claim_sat: u64,
    pub miner_fee_server_sat: u64,
    pub fixed_miner_fee_total_sat: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzChainSwapPairsInfo {
    pub bitcoin_to_liquid: BoltzChainSwapPairInfo,
    pub liquid_to_bitcoin: BoltzChainSwapPairInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltzSwapStatusResponse {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lockup_txid: Option<String>,
}

impl BoltzService {
    pub fn new(network: Network, boltz_api_url_override: Option<String>) -> Self {
        let boltz_api_url = boltz_api_url_override.unwrap_or_else(|| default_api_url(network));
        let client = BoltzApiClientV2::new(boltz_api_url.clone(), Some(Duration::from_secs(15)));
        Self {
            client,
            network,
            boltz_api_url,
        }
    }

    pub async fn create_submarine_swap(
        &self,
        invoice: &str,
        refund_pubkey_hex: &str,
    ) -> Result<BoltzSubmarineSwapCreated, PaymentError> {
        let amount_sat = parse_invoice_amount_sat(invoice)?;
        let (invoice_expiry_seconds, invoice_expires_at) = parse_invoice_expiry(invoice)?;
        let refund_public_key = BoltzPublicKey::from_str(refund_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid refund public key: {}", e))
        })?;

        let pairs = self
            .client
            .get_submarine_pairs()
            .await
            .map_err(map_boltz_err)?;
        let pair = pairs.get_lbtc_to_btc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return an L-BTC -> BTC submarine pair for this network".to_string(),
            )
        })?;
        pair.limits.within(amount_sat).map_err(map_boltz_err)?;
        let pair_hash = pair.hash.clone();

        let req = CreateSubmarineRequest {
            from: "L-BTC".to_string(),
            to: "BTC".to_string(),
            invoice: invoice.to_string(),
            refund_public_key,
            pair_hash: Some(pair_hash.clone()),
            referral_id: None,
            webhook: None,
        };

        let response = self
            .client
            .post_swap_req(&req)
            .await
            .map_err(map_boltz_err)?;
        response
            .validate(
                invoice,
                &req.refund_public_key,
                boltz_liquid_chain(self.network),
            )
            .map_err(map_boltz_err)?;

        Ok(BoltzSubmarineSwapCreated {
            id: response.id,
            flow: "liquid_to_lightning".to_string(),
            network: self.network.as_str().to_string(),
            boltz_api_url: self.boltz_api_url.clone(),
            status: "swap.created".to_string(),
            invoice_amount_sat: amount_sat,
            expected_amount_sat: response.expected_amount,
            lockup_address: response.address,
            timeout_block_height: response.timeout_block_height,
            pair_hash,
            bip21: response.bip21,
            invoice_expiry_seconds,
            invoice_expires_at,
        })
    }

    pub async fn create_lightning_receive(
        &self,
        amount_sat: u64,
        claim_pubkey_hex: &str,
    ) -> Result<BoltzLightningReceiveCreated, PaymentError> {
        if amount_sat == 0 {
            return Err(PaymentError::InvalidParameters(
                "Amount must be greater than zero".to_string(),
            ));
        }

        let claim_public_key = BoltzPublicKey::from_str(claim_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid claim public key: {}", e))
        })?;
        let preimage = BoltzPreimage::new();

        let pairs = self
            .client
            .get_reverse_pairs()
            .await
            .map_err(map_boltz_err)?;
        let pair = pairs.get_btc_to_lbtc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return a BTC -> L-BTC reverse pair for this network".to_string(),
            )
        })?;
        pair.limits.within(amount_sat).map_err(map_boltz_err)?;
        let pair_hash = pair.hash.clone();

        let req = CreateReverseRequest {
            from: "BTC".to_string(),
            to: "L-BTC".to_string(),
            claim_public_key: claim_public_key.clone(),
            invoice: None,
            invoice_amount: Some(amount_sat),
            preimage_hash: Some(preimage.sha256),
            description: None,
            description_hash: None,
            address: None,
            address_signature: None,
            referral_id: None,
            webhook: None,
        };
        let response = self
            .client
            .post_reverse_req(req)
            .await
            .map_err(map_boltz_err)?;
        response
            .validate(
                &preimage,
                &claim_public_key,
                boltz_liquid_chain(self.network),
            )
            .map_err(map_boltz_err)?;

        let invoice = response.invoice.ok_or_else(|| {
            PaymentError::Network("Boltz reverse swap response was missing invoice".to_string())
        })?;
        let (invoice_expiry_seconds, invoice_expires_at) = parse_invoice_expiry(&invoice)?;

        Ok(BoltzLightningReceiveCreated {
            id: response.id,
            flow: "lightning_to_liquid".to_string(),
            network: self.network.as_str().to_string(),
            boltz_api_url: self.boltz_api_url.clone(),
            status: "swap.created".to_string(),
            invoice_amount_sat: amount_sat,
            expected_onchain_amount_sat: response.onchain_amount,
            lockup_address: response.lockup_address,
            timeout_block_height: u64::from(response.timeout_block_height),
            pair_hash,
            invoice,
            invoice_expiry_seconds,
            invoice_expires_at,
            preimage_hash: preimage.sha256.to_string(),
        })
    }

    pub async fn create_chain_swap_btc_to_lbtc(
        &self,
        amount_sat: u64,
        claim_pubkey_hex: &str,
        refund_pubkey_hex: &str,
    ) -> Result<BoltzChainSwapCreated, PaymentError> {
        if amount_sat == 0 {
            return Err(PaymentError::InvalidParameters(
                "Amount must be greater than zero".to_string(),
            ));
        }

        let claim_public_key = BoltzPublicKey::from_str(claim_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid claim public key: {}", e))
        })?;
        let refund_public_key = BoltzPublicKey::from_str(refund_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid refund public key: {}", e))
        })?;
        let preimage = BoltzPreimage::new();

        let pairs = self.client.get_chain_pairs().await.map_err(map_boltz_err)?;
        let pair = pairs.get_btc_to_lbtc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return a BTC -> L-BTC chain pair for this network".to_string(),
            )
        })?;
        let pair_hash = pair.hash.clone();

        let req = CreateChainRequest {
            from: "BTC".to_string(),
            to: "L-BTC".to_string(),
            preimage_hash: preimage.sha256,
            claim_public_key: Some(claim_public_key.clone()),
            refund_public_key: Some(refund_public_key.clone()),
            user_lock_amount: Some(amount_sat),
            server_lock_amount: None,
            pair_hash: Some(pair_hash.clone()),
            referral_id: None,
            webhook: None,
        };

        let response = self
            .client
            .post_chain_req(req)
            .await
            .map_err(map_boltz_err)?;
        response
            .validate(
                &claim_public_key,
                &refund_public_key,
                boltz_bitcoin_chain(self.network),
                boltz_liquid_chain(self.network),
            )
            .map_err(map_boltz_err)?;

        Ok(BoltzChainSwapCreated {
            id: response.id,
            flow: "bitcoin_to_liquid".to_string(),
            network: self.network.as_str().to_string(),
            boltz_api_url: self.boltz_api_url.clone(),
            status: "swap.created".to_string(),
            amount_sat,
            expected_amount_sat: response.claim_details.amount,
            lockup_address: response.lockup_details.lockup_address,
            claim_lockup_address: response.claim_details.lockup_address,
            timeout_block_height: u64::from(response.lockup_details.timeout_block_height),
            pair_hash,
            bip21: response.lockup_details.bip21,
            preimage_hash: preimage.sha256.to_string(),
        })
    }

    pub async fn create_chain_swap_lbtc_to_btc(
        &self,
        amount_sat: u64,
        claim_pubkey_hex: &str,
        refund_pubkey_hex: &str,
    ) -> Result<BoltzChainSwapCreated, PaymentError> {
        if amount_sat == 0 {
            return Err(PaymentError::InvalidParameters(
                "Amount must be greater than zero".to_string(),
            ));
        }

        let claim_public_key = BoltzPublicKey::from_str(claim_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid claim public key: {}", e))
        })?;
        let refund_public_key = BoltzPublicKey::from_str(refund_pubkey_hex).map_err(|e| {
            PaymentError::InvalidParameters(format!("Invalid refund public key: {}", e))
        })?;
        let preimage = BoltzPreimage::new();

        let pairs = self.client.get_chain_pairs().await.map_err(map_boltz_err)?;
        let pair = pairs.get_lbtc_to_btc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return an L-BTC -> BTC chain pair for this network".to_string(),
            )
        })?;
        let pair_hash = pair.hash.clone();

        let req = CreateChainRequest {
            from: "L-BTC".to_string(),
            to: "BTC".to_string(),
            preimage_hash: preimage.sha256,
            claim_public_key: Some(claim_public_key.clone()),
            refund_public_key: Some(refund_public_key.clone()),
            user_lock_amount: Some(amount_sat),
            server_lock_amount: None,
            pair_hash: Some(pair_hash.clone()),
            referral_id: None,
            webhook: None,
        };

        let response = self
            .client
            .post_chain_req(req)
            .await
            .map_err(map_boltz_err)?;
        response
            .validate(
                &claim_public_key,
                &refund_public_key,
                boltz_liquid_chain(self.network),
                boltz_bitcoin_chain(self.network),
            )
            .map_err(map_boltz_err)?;

        Ok(BoltzChainSwapCreated {
            id: response.id,
            flow: "liquid_to_bitcoin".to_string(),
            network: self.network.as_str().to_string(),
            boltz_api_url: self.boltz_api_url.clone(),
            status: "swap.created".to_string(),
            amount_sat,
            expected_amount_sat: response.claim_details.amount,
            lockup_address: response.lockup_details.lockup_address,
            claim_lockup_address: response.claim_details.lockup_address,
            timeout_block_height: u64::from(response.lockup_details.timeout_block_height),
            pair_hash,
            bip21: response.lockup_details.bip21,
            preimage_hash: preimage.sha256.to_string(),
        })
    }

    pub async fn get_swap_status(&self, id: &str) -> Result<BoltzSwapStatusResponse, PaymentError> {
        let swap = self.client.get_swap(id).await.map_err(map_boltz_err)?;
        Ok(BoltzSwapStatusResponse {
            id: id.to_string(),
            status: swap.status,
            lockup_txid: swap.transaction.map(|tx| tx.id),
        })
    }

    pub async fn get_chain_swap_pairs_info(&self) -> Result<BoltzChainSwapPairsInfo, PaymentError> {
        let pairs = self.client.get_chain_pairs().await.map_err(map_boltz_err)?;

        let btc_to_lbtc = pairs.get_btc_to_lbtc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return a BTC -> L-BTC chain pair for this network".to_string(),
            )
        })?;
        let lbtc_to_btc = pairs.get_lbtc_to_btc_pair().ok_or_else(|| {
            PaymentError::Network(
                "Boltz did not return an L-BTC -> BTC chain pair for this network".to_string(),
            )
        })?;

        Ok(BoltzChainSwapPairsInfo {
            bitcoin_to_liquid: map_chain_pair_info(&btc_to_lbtc),
            liquid_to_bitcoin: map_chain_pair_info(&lbtc_to_btc),
        })
    }
}

fn default_api_url(network: Network) -> String {
    match network {
        Network::Mainnet => BOLTZ_MAINNET_URL_V2.to_string(),
        Network::Testnet => BOLTZ_TESTNET_URL_V2.to_string(),
        Network::Regtest => BOLTZ_REGTEST.to_string(),
    }
}

fn boltz_liquid_chain(network: Network) -> BoltzChain {
    let chain = match network {
        Network::Mainnet => LiquidChain::Liquid,
        Network::Testnet => LiquidChain::LiquidTestnet,
        Network::Regtest => LiquidChain::LiquidRegtest,
    };
    chain.into()
}

fn boltz_bitcoin_chain(network: Network) -> BoltzChain {
    let chain = match network {
        Network::Mainnet => BitcoinChain::Bitcoin,
        Network::Testnet => BitcoinChain::BitcoinTestnet,
        Network::Regtest => BitcoinChain::BitcoinRegtest,
    };
    chain.into()
}

fn map_chain_pair_info(pair: &boltz_client::swaps::boltz::ChainPair) -> BoltzChainSwapPairInfo {
    let miner_fee_lockup_sat = pair.fees.miner_fees.user.lockup;
    let miner_fee_claim_sat = pair.fees.miner_fees.user.claim;
    let miner_fee_server_sat = pair.fees.miner_fees.server;
    BoltzChainSwapPairInfo {
        pair_hash: pair.hash.clone(),
        min_amount_sat: pair.limits.minimal,
        max_amount_sat: pair.limits.maximal,
        fee_percentage: pair.fees.percentage,
        miner_fee_lockup_sat,
        miner_fee_claim_sat,
        miner_fee_server_sat,
        fixed_miner_fee_total_sat: miner_fee_claim_sat + miner_fee_server_sat,
    }
}

fn map_boltz_err(err: boltz_client::error::Error) -> PaymentError {
    PaymentError::Network(format!("Boltz API error: {}", err))
}

fn parse_invoice_amount_sat(invoice: &str) -> Result<u64, PaymentError> {
    let invoice = Bolt11Invoice::from_str(invoice)
        .map_err(|e| PaymentError::InvalidParameters(format!("Invalid BOLT11 invoice: {}", e)))?;
    let amount_msat = invoice.amount_milli_satoshis().ok_or_else(|| {
        PaymentError::InvalidParameters(
            "Invoice is missing amount (zero-amount invoices are not yet supported)".to_string(),
        )
    })?;
    Ok(amount_msat.div_ceil(1_000))
}

fn parse_invoice_expiry(invoice: &str) -> Result<(u64, String), PaymentError> {
    let invoice = Bolt11Invoice::from_str(invoice)
        .map_err(|e| PaymentError::InvalidParameters(format!("Invalid BOLT11 invoice: {}", e)))?;
    let expiry_seconds = invoice.expiry_time().as_secs();
    let expires_at = invoice.expires_at().ok_or_else(|| {
        PaymentError::InvalidParameters("Invoice expiry timestamp overflow".to_string())
    })?;
    let expires_at_unix = i64::try_from(expires_at.as_secs())
        .map_err(|_| PaymentError::InvalidParameters("Invoice expiry out of range".to_string()))?;
    let expires_at_rfc3339 = chrono::Utc
        .timestamp_opt(expires_at_unix, 0)
        .single()
        .ok_or_else(|| {
            PaymentError::InvalidParameters("Invalid invoice expiry timestamp".to_string())
        })?
        .to_rfc3339();
    Ok((expiry_seconds, expires_at_rfc3339))
}
