use lwk_wollet::elements::Txid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DormantOutputOpening {
    pub asset_blinding_factor: String,
    pub value_blinding_factor: String,
}

impl DormantOutputOpening {
    pub fn from_bytes(asset_blinding_factor: [u8; 32], value_blinding_factor: [u8; 32]) -> Self {
        Self {
            asset_blinding_factor: hex::encode(asset_blinding_factor),
            value_blinding_factor: hex::encode(value_blinding_factor),
        }
    }

    pub fn parse(&self, field: &str) -> Result<ParsedDormantOutputOpening, String> {
        Ok(ParsedDormantOutputOpening {
            asset_blinding_factor: parse_hex32(
                &format!("{field}.asset_blinding_factor"),
                &self.asset_blinding_factor,
            )?,
            value_blinding_factor: parse_hex32(
                &format!("{field}.value_blinding_factor"),
                &self.value_blinding_factor,
            )?,
        })
    }

    pub fn canonicalized(&self, field: &str) -> Result<Self, String> {
        let parsed = self.parse(field)?;
        Ok(Self::from_bytes(
            parsed.asset_blinding_factor,
            parsed.value_blinding_factor,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredictionMarketAnchor {
    pub creation_txid: String,
    pub yes_dormant_opening: DormantOutputOpening,
    pub no_dormant_opening: DormantOutputOpening,
}

impl PredictionMarketAnchor {
    pub fn from_openings(
        creation_txid: Txid,
        yes_asset_blinding_factor: [u8; 32],
        yes_value_blinding_factor: [u8; 32],
        no_asset_blinding_factor: [u8; 32],
        no_value_blinding_factor: [u8; 32],
    ) -> Self {
        Self {
            creation_txid: creation_txid.to_string(),
            yes_dormant_opening: DormantOutputOpening::from_bytes(
                yes_asset_blinding_factor,
                yes_value_blinding_factor,
            ),
            no_dormant_opening: DormantOutputOpening::from_bytes(
                no_asset_blinding_factor,
                no_value_blinding_factor,
            ),
        }
    }

    pub fn parse(&self) -> Result<ParsedPredictionMarketAnchor, String> {
        Ok(ParsedPredictionMarketAnchor {
            creation_txid: parse_market_creation_txid(&self.creation_txid)?,
            yes_dormant_opening: self.yes_dormant_opening.parse("yes_dormant_opening")?,
            no_dormant_opening: self.no_dormant_opening.parse("no_dormant_opening")?,
        })
    }

    pub fn canonicalized(&self) -> Result<Self, String> {
        let parsed = self.parse()?;
        Ok(Self {
            creation_txid: parsed.creation_txid.to_string(),
            yes_dormant_opening: DormantOutputOpening::from_bytes(
                parsed.yes_dormant_opening.asset_blinding_factor,
                parsed.yes_dormant_opening.value_blinding_factor,
            ),
            no_dormant_opening: DormantOutputOpening::from_bytes(
                parsed.no_dormant_opening.asset_blinding_factor,
                parsed.no_dormant_opening.value_blinding_factor,
            ),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedDormantOutputOpening {
    pub asset_blinding_factor: [u8; 32],
    pub value_blinding_factor: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedPredictionMarketAnchor {
    pub creation_txid: Txid,
    pub yes_dormant_opening: ParsedDormantOutputOpening,
    pub no_dormant_opening: ParsedDormantOutputOpening,
}

/// Parse and canonicalize a prediction-market creation txid string.
#[doc(hidden)]
pub fn parse_market_creation_txid(creation_txid: &str) -> Result<Txid, String> {
    let trimmed = creation_txid.trim();
    if trimmed.is_empty() {
        return Err("missing required creation_txid".to_string());
    }

    let txid = trimmed
        .parse::<Txid>()
        .map_err(|e| format!("invalid creation_txid '{trimmed}': {e}"))?;

    if trimmed != txid.to_string() {
        return Err(format!(
            "invalid creation_txid '{trimmed}': must use canonical lowercase hex"
        ));
    }

    Ok(txid)
}

#[doc(hidden)]
pub fn parse_prediction_market_anchor(
    anchor: &PredictionMarketAnchor,
) -> Result<ParsedPredictionMarketAnchor, String> {
    anchor.parse()
}

fn parse_hex32(field: &str, value: &str) -> Result<[u8; 32], String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("missing required {field}"));
    }

    let decoded = hex::decode(trimmed).map_err(|e| format!("invalid {field} '{trimmed}': {e}"))?;
    let array: [u8; 32] = decoded
        .try_into()
        .map_err(|_| format!("invalid {field} '{trimmed}': expected 32-byte lowercase hex"))?;

    let canonical = hex::encode(array);
    if trimmed != canonical {
        return Err(format!(
            "invalid {field} '{trimmed}': must use canonical lowercase hex"
        ));
    }

    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwk_wollet::elements::hashes::Hash as _;

    fn valid_anchor() -> PredictionMarketAnchor {
        PredictionMarketAnchor::from_openings(
            Txid::from_byte_array([0x11; 32]),
            [0x21; 32],
            [0x31; 32],
            [0x41; 32],
            [0x51; 32],
        )
    }

    fn set_field(anchor: &mut PredictionMarketAnchor, field: &str, value: &str) {
        match field {
            "creation_txid" => anchor.creation_txid = value.to_string(),
            "yes_dormant_opening.asset_blinding_factor" => {
                anchor.yes_dormant_opening.asset_blinding_factor = value.to_string()
            }
            "yes_dormant_opening.value_blinding_factor" => {
                anchor.yes_dormant_opening.value_blinding_factor = value.to_string()
            }
            "no_dormant_opening.asset_blinding_factor" => {
                anchor.no_dormant_opening.asset_blinding_factor = value.to_string()
            }
            "no_dormant_opening.value_blinding_factor" => {
                anchor.no_dormant_opening.value_blinding_factor = value.to_string()
            }
            other => panic!("unexpected field: {other}"),
        }
    }

    #[test]
    fn prediction_market_anchor_parses_and_canonicalizes() {
        let anchor = valid_anchor();
        let parsed = parse_prediction_market_anchor(&anchor).unwrap();
        assert_eq!(parsed.creation_txid, Txid::from_byte_array([0x11; 32]));
        assert_eq!(anchor.canonicalized().unwrap(), valid_anchor(),);
    }

    #[test]
    fn prediction_market_anchor_rejects_invalid_creation_txid() {
        let mut anchor = valid_anchor();
        anchor.creation_txid = "not-a-txid".to_string();
        let err = parse_prediction_market_anchor(&anchor).unwrap_err();
        assert!(err.contains("invalid creation_txid"));
    }

    #[test]
    fn prediction_market_anchor_rejects_malformed_opening_fields() {
        let cases = [
            (
                "yes_dormant_opening.asset_blinding_factor",
                "zz".to_string(),
            ),
            ("yes_dormant_opening.asset_blinding_factor", "11".repeat(31)),
            ("yes_dormant_opening.asset_blinding_factor", "AB".repeat(32)),
            ("yes_dormant_opening.asset_blinding_factor", "".to_string()),
            (
                "yes_dormant_opening.value_blinding_factor",
                "zz".to_string(),
            ),
            ("yes_dormant_opening.value_blinding_factor", "12".repeat(31)),
            ("yes_dormant_opening.value_blinding_factor", "CD".repeat(32)),
            ("yes_dormant_opening.value_blinding_factor", "".to_string()),
            ("no_dormant_opening.asset_blinding_factor", "zz".to_string()),
            ("no_dormant_opening.asset_blinding_factor", "21".repeat(31)),
            ("no_dormant_opening.asset_blinding_factor", "EF".repeat(32)),
            ("no_dormant_opening.asset_blinding_factor", "".to_string()),
            ("no_dormant_opening.value_blinding_factor", "zz".to_string()),
            ("no_dormant_opening.value_blinding_factor", "22".repeat(31)),
            ("no_dormant_opening.value_blinding_factor", "AA".repeat(32)),
            ("no_dormant_opening.value_blinding_factor", "".to_string()),
        ];

        for (field, value) in cases {
            let mut anchor = valid_anchor();
            set_field(&mut anchor, field, &value);
            let err = parse_prediction_market_anchor(&anchor).unwrap_err();
            assert!(
                err.contains(field),
                "expected field-specific error for {field}, got: {err}"
            );
        }
    }
}
