use std::time::Duration;
use std::cmp::max;
use std::fs::File;
use std::io::BufReader;
use subxt::{
    tx::{
        Era,
        PairSigner,
        PlainTip,
        PolkadotExtrinsicParamsBuilder as Params,
    },
    ext::{
        sp_core::{sr25519, Pair},
        sp_runtime::AccountId32,
    },
    OnlineClient,
    PolkadotConfig,
};
use serde::Deserialize;

#[subxt::subxt(runtime_metadata_path = "./data/metadata.scale")]
pub mod polkadot {}

pub mod balances;
pub mod council;
pub mod staking;
pub mod democracy;
pub mod consts;
use consts::*;

type Call = polkadot::runtime_types::edgeware_runtime::Call;
type BalancesCall = polkadot::runtime_types::pallet_balances::pallet::Call;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Config {
    sudo_seed: String,
    host: String,

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let file = File::open("./data/config.json")?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader).expect("Something went wrong reading the configuration file");
    let api = OnlineClient::<PolkadotConfig>::from_url(config.host).await?;
    let sudo_seed_account = sr25519::Pair::from_string(&config.sudo_seed, None).expect("constructed from known-good static value; qed");
    let sudo_seed_account_id: AccountId32 = sudo_seed_account.public().into();
    let sudo_signer = PairSigner::new(sudo_seed_account.clone());
    // Fund the sudo account
    let sudo_free = max(1_000_000_001_000 * EDG, TEST_ACCOUNT_FUNDING * (NB_TEST_ACCOUNTS + 1) as u128);
    let call = Call::Balances(
        BalancesCall::set_balance {
            who: sudo_seed_account_id.into(),
            new_free: sudo_free,
            new_reserved: 1_000 * EDG
        }
    );
    let tx = polkadot::tx().sudo().sudo(call);
    let found_event = api
    .tx()
        .sign_and_submit_then_watch_default(&tx, &sudo_signer)
        .await?
        .wait_for_finalized_success()
        .await?
        .has::<polkadot::sudo::events::Sudid>()?;
    assert!(found_event);
    println!("Funded the sudo account");
    // Create and fund test accounts.
    let mut acc_seed_accounts = Vec::new();
    for i in 0..NB_TEST_ACCOUNTS {
        let acc_seed = config.sudo_seed.to_owned() + "//" + &i.to_string();
        let acc_seed_account = sr25519::Pair::from_string(&acc_seed, None).expect("constructed from known-good static value; qed");
        let acc_seed_account_id: AccountId32 = acc_seed_account.public().into();
        acc_seed_accounts.push(acc_seed_account);
        let tx = polkadot::tx()
        .balances()
        .transfer(acc_seed_account_id.into(), TEST_ACCOUNT_FUNDING);
        let tx_params = Params::new()
        .tip(PlainTip::new(0))
        .era(Era::Immortal, api.genesis_hash());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &sudo_signer, tx_params).await?;
        println!("Balance transfer extrinsic submitted for test account {}: {}",i, hash);
    }
    // Nominate validators and set up a council
    staking::nominate_all(&api, &acc_seed_accounts[..]).await?;
    council::populate_council(&api, &acc_seed_accounts[..]).await?;
    let referendum_storage_index = polkadot::storage().democracy().referendum_count();
    // Propose the upgrade through democracy
    democracy::propose_upgrade(&api, &acc_seed_accounts[..]).await?;
    tokio::time::sleep(Duration::from_secs(60+BLOCK_INCLUSION_LAG)).await;
    let referendum_index = api.storage().fetch(&referendum_storage_index, None).await?;
    let referendum_index = if let Some(t) = referendum_index {
        if 0 == t {
            panic!("ERROR: Democracy proposal incorrectly registered");
        }
        t-1
    } else {
        panic!("ERROR: Democracy proposal incorrectly registered");
    };
    // Record all the balances data
    let account_data_before = balances::dump_balances(&api).await?;
    // Approve the upgrade
    democracy::vote(&api, &acc_seed_accounts[..], referendum_index, true).await?;
    tokio::time::sleep(Duration::from_secs(60*3)).await;
    // Assume the network is upgraded.
    // Verify the balances. Verify the staking, unbonding and the council elections.
    // Record all the balances after upgrade
    let account_data = balances::dump_balances(&api).await?;
    for (a, (b0,b1,b2,b3)) in account_data {
        if let Some((b0_,b1_, b2_, b3_)) = account_data_before.get(&a) {
            let b= b0+b1+b2+b3;
            let b_ = *b0_+*b1_+*b2_+*b3_;
            if b!=b_ {
                println!("### Balances of account {} do not match ###",a);
                println!("Difference in EDG: {} before/after {} / {}.",(b/EDG) as f64-(b_/EDG) as f64, b_,b);
                println!("Difference in EDG: free {} before/after {} / {}.",(b0/EDG) as f64-(b0_/EDG) as f64, b0_,b0);
                println!("Difference in EDG: reserved {} before/after {} / {}.",(b1/EDG) as f64-(b1_/EDG) as f64, b1_,b1);
                println!("Difference in EDG: mix_frozen {} before/after {} / {}.",(b2/EDG) as f64-(b2_/EDG) as f64, b2_,b2);
                println!("Difference in EDG: fee_frozen {} before/after {} / {}.",(b3/EDG) as f64-(b3_/EDG) as f64, b3_,b3);
            }
        }else{
            println!("Balances of account {} have been created: {} {} {} {}.",a, b0,b1,b2,b3);
        }
    }
    council::external_majority_workflow(&api, &acc_seed_accounts[..]).await?;
    tokio::time::sleep(Duration::from_secs(60+BLOCK_INCLUSION_LAG)).await;
    let referendum_index = api.storage().fetch(&referendum_storage_index, None).await?;
    let referendum_index = if let Some(t) = referendum_index {
        if 0 == t {
            panic!("ERROR: Democracy proposal incorrectly registered");
        }
        t-1
    } else {
        panic!("ERROR: Democracy proposal incorrectly registered");
    };
    democracy::vote(&api, &acc_seed_accounts[..], referendum_index, false).await?;
    Ok(())
}
