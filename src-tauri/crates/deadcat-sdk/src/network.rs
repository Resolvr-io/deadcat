use lwk_wollet::ElementsNetwork;
use lwk_wollet::elements::AddressParams;
use serde::Deserialize;

/// Network variants for Liquid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Liquid,
    LiquidTestnet,
    LiquidRegtest,
}

impl Network {
    pub fn into_lwk(self) -> ElementsNetwork {
        match self {
            Network::Liquid => ElementsNetwork::Liquid,
            Network::LiquidTestnet => ElementsNetwork::LiquidTestnet,
            Network::LiquidRegtest => ElementsNetwork::default_regtest(),
        }
    }

    pub fn is_mainnet(self) -> bool {
        matches!(self, Network::Liquid)
    }

    pub fn default_electrum_url(self) -> &'static str {
        match self {
            Network::Liquid => "ssl://blockstream.info:995",
            Network::LiquidTestnet => "ssl://blockstream.info:465",
            Network::LiquidRegtest => "tcp://localhost:50001",
        }
    }

    pub fn esplora_url(self) -> &'static str {
        match self {
            Network::Liquid => "https://blockstream.info/liquid/api",
            Network::LiquidTestnet => "https://blockstream.info/liquidtestnet/api",
            Network::LiquidRegtest => "http://localhost:3000",
        }
    }

    pub fn address_params(self) -> &'static AddressParams {
        match self {
            Network::Liquid => &AddressParams::LIQUID,
            Network::LiquidTestnet => &AddressParams::LIQUID_TESTNET,
            Network::LiquidRegtest => &AddressParams::LIQUID_TESTNET,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Network::Liquid => "mainnet",
            Network::LiquidTestnet => "testnet",
            Network::LiquidRegtest => "regtest",
        }
    }
}

impl std::str::FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mainnet" | "liquid" => Ok(Network::Liquid),
            "testnet" | "liquid-testnet" | "liquidtestnet" => Ok(Network::LiquidTestnet),
            "regtest" | "liquid-regtest" | "liquidregtest" => Ok(Network::LiquidRegtest),
            _ => Err(format!("invalid network: {}", s)),
        }
    }
}
