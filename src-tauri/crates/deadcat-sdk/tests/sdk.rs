use deadcat_sdk::DeadcatSdk;
use lwk_signer::SwSigner;
use lwk_test_util::{
    TEST_MNEMONIC, TestEnv, TestEnvBuilder, generate_mnemonic, regtest_policy_asset,
};
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::Txid;
use lwk_wollet::{ElectrumClient, ElectrumUrl, Wollet};
use tempfile::TempDir;

use std::str::FromStr;
use std::time::Duration;

#[test]
fn test_the_sdk() {
    let test_fixture = TestFixture::new();
    let env = test_fixture.env;
    let mut sdk = test_fixture.sdk;

    let sdk_address_result = sdk.address(None).unwrap();
    let sdk_address = sdk_address_result.address();
    env.elementsd_sendtoaddress(sdk_address, 1234, None);
    env.elementsd_generate(10);
    let lbtc_asset = regtest_policy_asset();
    assert_eq!(*sdk.balance().unwrap().get(&lbtc_asset).unwrap(), 0);
    sdk.sync().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(*sdk.balance().unwrap().get(&lbtc_asset).unwrap(), 1234);
}

struct TestFixture {
    env: TestEnv,
    sdk: DeadcatSdk,
    _temp_dir: TempDir,
}

impl TestFixture {
    fn new() -> Self {
        let test_env = TestEnvBuilder::from_env().with_electrum().build();

        let temp_dir = tempfile::tempdir().unwrap();

        let sdk = DeadcatSdk::new(
            TEST_MNEMONIC,
            deadcat_sdk::Network::LiquidRegtest,
            &test_env.electrum_url(),
            temp_dir.path(),
        )
        .unwrap();

        TestFixture {
            env: test_env,
            sdk,
            _temp_dir: temp_dir,
        }
    }
}

pub fn generate_signer() -> SwSigner {
    let mnemonic = generate_mnemonic();
    SwSigner::new(&mnemonic, false).unwrap()
}

pub fn electrum_client(env: &TestEnv) -> ElectrumClient {
    let electrum_url = ElectrumUrl::from_str(&env.electrum_url()).unwrap();
    ElectrumClient::new(&electrum_url).unwrap()
}

pub fn sync<S: BlockchainBackend>(wollet: &mut Wollet, client: &mut S) {
    let update = client.full_scan(wollet).unwrap();
    if let Some(update) = update {
        wollet.apply_update(update).unwrap();
    }
}

pub fn wait_for_tx<S: BlockchainBackend>(wollet: &mut Wollet, client: &mut S, txid: &Txid) {
    for _ in 0..120 {
        sync(wollet, client);
        let list = wollet.transactions().unwrap();
        if list.iter().any(|e| &e.txid == txid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    panic!("Wallet does not have {txid} in its list");
}
