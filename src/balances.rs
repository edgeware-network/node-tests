use std::collections::HashMap;
use subxt::{
    OnlineClient,
    PolkadotConfig,
};

#[subxt::subxt(runtime_metadata_path = "./data/metadata.scale")]
pub mod polkadot {}

pub async fn dump_balances(api: &OnlineClient<PolkadotConfig>) -> Result<HashMap<String, (u128,u128,u128,u128)>, Box<dyn std::error::Error>> {
    let address = polkadot::storage().system().account_root();
    let mut iter = api.storage().iter(address, 10, None).await?;
    let mut data = HashMap::new();
    while let Some((key, account)) = iter.next().await? {
        data.insert(hex::encode(key), (account.data.free,account.data.reserved,account.data.misc_frozen,account.data.fee_frozen));
    }
    Ok(data)
}