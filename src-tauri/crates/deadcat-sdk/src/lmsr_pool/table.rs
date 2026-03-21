use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

use super::params::LmsrPoolParams;

const LMSR_TABLE_LEAF_DOMAIN: &[u8] = b"LMSR_TBL_V1";

/// Compute canonical LMSR table leaf hash:
/// `SHA256(0x00 || "LMSR_TBL_V1" || be64(index) || be64(value))`.
pub fn lmsr_table_leaf_hash(index: u64, value: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x00]);
    hasher.update(LMSR_TABLE_LEAF_DOMAIN);
    hasher.update(index.to_be_bytes());
    hasher.update(value.to_be_bytes());
    hasher.finalize().into()
}

/// Compute canonical LMSR table internal node hash:
/// `SHA256(0x01 || left || right)`.
pub fn lmsr_table_node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x01]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Compute canonical LMSR table root from ordered values `F(i)`.
pub fn lmsr_table_root(values: &[u64]) -> Result<[u8; 32]> {
    if values.is_empty() {
        return Err(Error::LmsrPool("LMSR table values cannot be empty".into()));
    }
    if !values.len().is_power_of_two() {
        return Err(Error::LmsrPool(format!(
            "LMSR table leaf count must be power-of-two, got {}",
            values.len()
        )));
    }

    let mut level: Vec<[u8; 32]> = values
        .iter()
        .enumerate()
        .map(|(i, v)| lmsr_table_leaf_hash(i as u64, *v))
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            next.push(lmsr_table_node_hash(&pair[0], &pair[1]));
        }
        level = next;
    }
    Ok(level[0])
}

/// In-memory LMSR table manifest used by quote logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LmsrTableManifest {
    pub table_depth: u32,
    pub values: Vec<u64>,
}

/// Canonical LMSR table Merkle proof for one `(index, value)` leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LmsrTableProof {
    pub index: u64,
    pub value: u64,
    pub path_bits: u64,
    pub siblings: Vec<[u8; 32]>,
}

impl LmsrTableManifest {
    /// Build a manifest and validate depth/length consistency.
    pub fn new(table_depth: u32, values: Vec<u64>) -> Result<Self> {
        if table_depth == 0 || table_depth > 63 {
            return Err(Error::LmsrPool(format!(
                "table_depth must be in [1, 63], got {table_depth}"
            )));
        }
        let expected_len = expected_leaf_count(table_depth)?;
        if values.len() != expected_len {
            return Err(Error::LmsrPool(format!(
                "table depth {table_depth} expects {expected_len} values, got {}",
                values.len()
            )));
        }
        Ok(Self {
            table_depth,
            values,
        })
    }

    pub fn max_index(&self) -> u64 {
        self.values.len() as u64 - 1
    }

    pub fn value_at(&self, index: u64) -> Result<u64> {
        self.values
            .get(index as usize)
            .copied()
            .ok_or_else(|| Error::LmsrPool(format!("table index {index} out of range")))
    }

    pub fn root(&self) -> Result<[u8; 32]> {
        lmsr_table_root(&self.values)
    }

    /// Build a canonical Merkle proof for `F(index)`.
    pub fn proof_at(&self, index: u64) -> Result<LmsrTableProof> {
        let idx = usize::try_from(index)
            .map_err(|_| Error::LmsrPool(format!("table index {index} does not fit usize")))?;
        let value = *self
            .values
            .get(idx)
            .ok_or_else(|| Error::LmsrPool(format!("table index {index} out of range")))?;

        let depth = self.table_depth as usize;
        let mut siblings = Vec::with_capacity(depth);
        let mut path_bits = 0u64;
        let mut pos = idx;
        let mut level: Vec<[u8; 32]> = self
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| lmsr_table_leaf_hash(i as u64, *v))
            .collect();

        for d in 0..depth {
            let is_right = (pos & 1) == 1;
            if is_right {
                path_bits |= 1u64 << d;
            }
            let sibling_index = if is_right { pos - 1 } else { pos + 1 };
            let sibling = *level.get(sibling_index).ok_or_else(|| {
                Error::LmsrPool(format!(
                    "failed to derive LMSR sibling at depth {d} (pos {pos}, level size {})",
                    level.len()
                ))
            })?;
            siblings.push(sibling);

            let mut next = Vec::with_capacity(level.len() / 2);
            for pair in level.chunks_exact(2) {
                next.push(lmsr_table_node_hash(&pair[0], &pair[1]));
            }
            level = next;
            pos /= 2;
        }

        Ok(LmsrTableProof {
            index,
            value,
            path_bits,
            siblings,
        })
    }

    /// Verify this manifest is compatible with pool parameters.
    pub fn verify_matches_pool_params(&self, params: &LmsrPoolParams) -> Result<()> {
        params
            .validate()
            .map_err(|e| Error::LmsrPool(format!("invalid LMSR params: {e}")))?;
        if self.table_depth != params.table_depth {
            return Err(Error::LmsrPool(format!(
                "manifest depth {} does not match pool table_depth {}",
                self.table_depth, params.table_depth
            )));
        }
        if self.max_index() < params.s_max_index {
            return Err(Error::LmsrPool(format!(
                "manifest max index {} below pool s_max_index {}",
                self.max_index(),
                params.s_max_index
            )));
        }
        let root = self.root()?;
        if root != params.lmsr_table_root {
            return Err(Error::LmsrPool(format!(
                "manifest root {} does not match pool LMSR_TABLE_ROOT {}",
                hex::encode(root),
                hex::encode(params.lmsr_table_root)
            )));
        }
        Ok(())
    }
}

/// Verify a canonical LMSR Merkle proof against a committed table root.
pub fn verify_lmsr_table_proof(
    root: [u8; 32],
    table_depth: u32,
    index: u64,
    value: u64,
    path_bits: u64,
    siblings: &[[u8; 32]],
) -> Result<()> {
    let depth = table_depth as usize;
    if siblings.len() != depth {
        return Err(Error::LmsrPool(format!(
            "expected {depth} LMSR siblings, got {}",
            siblings.len()
        )));
    }
    let max_index = (1u128 << table_depth) - 1;
    if (index as u128) > max_index {
        return Err(Error::LmsrPool(format!(
            "proof index {index} exceeds table depth {table_depth} max {max_index}"
        )));
    }
    let depth_mask = if table_depth == 64 {
        u64::MAX
    } else {
        (1u64 << table_depth) - 1
    };
    if (path_bits & depth_mask) != index {
        return Err(Error::LmsrPool(format!(
            "proof path_bits {} does not match index {} for depth {}",
            path_bits, index, table_depth
        )));
    }
    if (path_bits & !depth_mask) != 0 {
        return Err(Error::LmsrPool(format!(
            "proof path_bits {} has non-zero bits above table_depth {}",
            path_bits, table_depth
        )));
    }

    let mut node = lmsr_table_leaf_hash(index, value);
    for (level, sibling) in siblings.iter().enumerate() {
        let is_right = ((path_bits >> level) & 1) == 1;
        node = if is_right {
            lmsr_table_node_hash(sibling, &node)
        } else {
            lmsr_table_node_hash(&node, sibling)
        };
    }

    if node != root {
        return Err(Error::LmsrPool(format!(
            "LMSR table proof root mismatch: computed {}, expected {}",
            hex::encode(node),
            hex::encode(root)
        )));
    }

    Ok(())
}

/// Generate LMSR table values F(i) for a binary market.
///
/// F(i) = floor(b * ln(exp(q_yes(i)/b) + exp(q_no(i)/b)))
///
/// where q_yes(i) = (i as i64 - s_bias as i64) * q_step_lots * half_payout_sats
///       q_no(i)  = -q_yes(i)  (symmetric binary market)
///       b = liquidity_param * half_payout_sats (scaling)
pub fn generate_lmsr_table(
    liquidity_param: f64,
    table_depth: u32,
    q_step_lots: u64,
    s_bias: u64,
    half_payout_sats: u64,
) -> Result<Vec<u64>> {
    // Cap at 20 for generation to avoid OOM (2^20 = 1M entries × 8 bytes ≈ 8 MB).
    // The manifest/proof code supports up to 63 for on-chain verification, but
    // generating that many values in memory is not practical.
    if table_depth == 0 || table_depth > 20 {
        return Err(Error::LmsrPool(format!(
            "table_depth must be in [1, 20] for generation, got {table_depth}"
        )));
    }
    if liquidity_param <= 0.0 || !liquidity_param.is_finite() {
        return Err(Error::LmsrPool(
            "liquidity_param must be a positive finite number".into(),
        ));
    }
    if half_payout_sats == 0 {
        return Err(Error::LmsrPool("half_payout_sats must be > 0".into()));
    }
    if q_step_lots == 0 {
        return Err(Error::LmsrPool("q_step_lots must be > 0".into()));
    }

    let n = 1usize << table_depth;
    let b = liquidity_param * half_payout_sats as f64;
    let mut values = Vec::with_capacity(n);

    for i in 0..n {
        let q_yes = (i as f64 - s_bias as f64) * q_step_lots as f64 * half_payout_sats as f64;
        let q_no = -q_yes;
        // F(i) = floor(b * ln(exp(q_yes/b) + exp(q_no/b)))
        // Use log-sum-exp trick for numerical stability:
        // ln(exp(a) + exp(b)) = max(a,b) + ln(1 + exp(-|a-b|))
        let a = q_yes / b;
        let a_neg = q_no / b;
        let max_val = a.max(a_neg);
        let lse = max_val + ((-((a - a_neg).abs())).exp()).ln_1p();
        let f_i = (b * lse).floor();
        if f_i < 0.0 || !f_i.is_finite() {
            return Err(Error::LmsrPool(format!(
                "LMSR table value at index {i} is invalid: {f_i}"
            )));
        }
        values.push(f_i as u64);
    }

    Ok(values)
}

fn expected_leaf_count(table_depth: u32) -> Result<usize> {
    let count_u128 = 1u128
        .checked_shl(table_depth)
        .ok_or_else(|| Error::LmsrPool(format!("table_depth {table_depth} is too large")))?;
    if count_u128 > usize::MAX as u128 {
        return Err(Error::LmsrPool(format!(
            "table_depth {table_depth} exceeds addressable host capacity"
        )));
    }
    Ok(count_u128 as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_values(depth: u32) -> Vec<u64> {
        let n = 1usize << depth;
        (0..n).map(|i| 1_000 + (i as u64 * 7)).collect()
    }

    fn sample_params_with_root(depth: u32, root: [u8; 32]) -> LmsrPoolParams {
        LmsrPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            lmsr_table_root: root,
            table_depth: depth,
            q_step_lots: 10,
            s_bias: 1_000,
            s_max_index: (1u64 << depth) - 1,
            half_payout_sats: 5_000,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        }
    }

    #[test]
    fn root_is_deterministic() {
        let values = sample_values(4);
        let r1 = lmsr_table_root(&values).unwrap();
        let r2 = lmsr_table_root(&values).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn manifest_rejects_bad_length() {
        let err = LmsrTableManifest::new(4, vec![1, 2, 3]).unwrap_err();
        assert!(err.to_string().contains("expects 16 values"));
    }

    #[test]
    fn manifest_validates_against_pool_params() {
        let values = sample_values(4);
        let root = lmsr_table_root(&values).unwrap();
        let manifest = LmsrTableManifest::new(4, values).unwrap();
        let params = sample_params_with_root(4, root);
        manifest.verify_matches_pool_params(&params).unwrap();
    }

    #[test]
    fn manifest_detects_root_mismatch() {
        let values = sample_values(4);
        let manifest = LmsrTableManifest::new(4, values).unwrap();
        let mut params = sample_params_with_root(4, [0x42; 32]);
        params.lmsr_table_root = [0x99; 32];
        let err = manifest.verify_matches_pool_params(&params).unwrap_err();
        assert!(err.to_string().contains("manifest root"));
    }

    #[test]
    fn proof_roundtrip_validates() {
        let values = sample_values(4);
        let root = lmsr_table_root(&values).unwrap();
        let manifest = LmsrTableManifest::new(4, values).unwrap();
        let proof = manifest.proof_at(7).unwrap();
        verify_lmsr_table_proof(
            root,
            manifest.table_depth,
            proof.index,
            proof.value,
            proof.path_bits,
            &proof.siblings,
        )
        .unwrap();
    }

    #[test]
    fn proof_rejects_wrong_value() {
        let values = sample_values(4);
        let root = lmsr_table_root(&values).unwrap();
        let manifest = LmsrTableManifest::new(4, values).unwrap();
        let proof = manifest.proof_at(3).unwrap();
        let err = verify_lmsr_table_proof(
            root,
            manifest.table_depth,
            proof.index,
            proof.value + 1,
            proof.path_bits,
            &proof.siblings,
        )
        .unwrap_err();
        assert!(err.to_string().contains("root mismatch"));
    }

    #[test]
    fn generate_lmsr_table_convexity() {
        let s_bias = 8u64;
        let values = generate_lmsr_table(1.0, 4, 10, s_bias, 5_000).unwrap();
        assert_eq!(values.len(), 16);
        // F(i) is convex (U-shaped): decreasing before s_bias, increasing after
        // Values should increase as we move away from s_bias
        for i in (s_bias as usize + 1)..values.len() {
            assert!(
                values[i] >= values[i - 1],
                "F({}) = {} < F({}) = {} (above bias)",
                i,
                values[i],
                i - 1,
                values[i - 1]
            );
        }
        for i in (1..=s_bias as usize).rev() {
            assert!(
                values[i - 1] >= values[i],
                "F({}) = {} < F({}) = {} (below bias)",
                i - 1,
                values[i - 1],
                i,
                values[i]
            );
        }
    }

    #[test]
    fn generate_lmsr_table_roundtrip_with_root() {
        let values = generate_lmsr_table(1.0, 3, 10, 4, 100).unwrap();
        assert_eq!(values.len(), 8);
        let root = lmsr_table_root(&values).unwrap();
        // Verify the root is deterministic
        let root2 = lmsr_table_root(&values).unwrap();
        assert_eq!(root, root2);
    }

    #[test]
    fn generate_lmsr_table_usable_with_quote_from_table() {
        use crate::lmsr_pool::math::{LmsrTradeKind, quote_from_table};

        let q_step_lots = 10u64;
        let half_payout_sats = 100u64;
        let fee_bps = 30u64;
        let values = generate_lmsr_table(1.0, 3, q_step_lots, 4, half_payout_sats).unwrap();

        // Quote a buy-yes trade from index 4 → 5
        let quote = quote_from_table(
            LmsrTradeKind::BuyYes,
            4,
            5,
            values[4],
            values[5],
            q_step_lots,
            half_payout_sats,
            fee_bps,
        )
        .unwrap();
        assert!(quote.collateral_amount > 0);
    }

    #[test]
    fn generate_lmsr_table_rejects_bad_depth() {
        let err = generate_lmsr_table(1.0, 0, 10, 4, 100).unwrap_err();
        assert!(err.to_string().contains("table_depth must be in [1, 20]"));
        let err = generate_lmsr_table(1.0, 21, 10, 4, 100).unwrap_err();
        assert!(err.to_string().contains("table_depth must be in [1, 20]"));
    }

    #[test]
    fn generate_lmsr_table_rejects_bad_liquidity() {
        let err = generate_lmsr_table(0.0, 3, 10, 4, 100).unwrap_err();
        assert!(err.to_string().contains("liquidity_param"));
        let err = generate_lmsr_table(-1.0, 3, 10, 4, 100).unwrap_err();
        assert!(err.to_string().contains("liquidity_param"));
    }

    #[test]
    fn generate_lmsr_table_symmetric_at_bias() {
        // At the bias point, q_yes = 0, q_no = 0, so F(s_bias) is the minimum
        let values = generate_lmsr_table(2.0, 4, 1, 8, 1_000).unwrap();
        // s_bias = 8 should be the minimum F value
        let min_val = *values.iter().min().unwrap();
        assert_eq!(values[8], min_val);
    }

    #[test]
    fn generate_lmsr_table_rejects_zero_q_step_lots() {
        let err = generate_lmsr_table(1.0, 3, 0, 4, 100).unwrap_err();
        assert!(err.to_string().contains("q_step_lots must be > 0"));
    }

    #[test]
    fn generate_lmsr_table_s_bias_beyond_table() {
        // s_bias = 100 but table only has 8 entries (depth=3).
        // All indices are below the bias, so all q_yes values are negative.
        // This is valid — the table is just lopsided.
        let values = generate_lmsr_table(1.0, 3, 10, 100, 5_000).unwrap();
        assert_eq!(values.len(), 8);
        // All values should be >= the value closest to bias (last entry)
        let last = values[7];
        for (i, &v) in values.iter().enumerate() {
            assert!(
                v >= last,
                "F({i}) = {v} < F(7) = {last} — expected decreasing toward bias"
            );
        }
    }
}
