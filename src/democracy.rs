use std::time::Duration;
use codec::Encode;
use subxt::{
    tx::{
        Era,
        PairSigner,
        PlainTip,
        PolkadotExtrinsicParamsBuilder as Params,
    },
    ext::{
        sp_core::sr25519,
        sp_runtime::traits::{BlakeTwo256, Hash},
    },
    OnlineClient,
    PolkadotConfig,
};
use rand::Rng;
use crate::consts::*;

#[subxt::subxt(runtime_metadata_path = "./data/metadata.scale")]
pub mod polkadot {}
use polkadot::runtime_types::pallet_democracy::vote::Vote;

type Call = polkadot::runtime_types::edgeware_runtime::Call;
type SystemCall = polkadot::runtime_types::frame_system::pallet::Call;

type AccountVote = polkadot::runtime_types::pallet_democracy::vote::AccountVote<::core::primitive::u128>;

/// A value denoting the strength of conviction of a vote.
#[derive(Encode, Clone, Copy)]
pub enum Conviction {
	/// 0.1x votes, unlocked.
	None,
	/// 1x votes, locked for an enactment period following a successful vote.
	Locked1x,
	/// 2x votes, locked for 2x enactment periods following a successful vote.
	Locked2x,
	/// 3x votes, locked for 4x...
	Locked3x,
	/// 4x votes, locked for 8x...
	Locked4x,
	/// 5x votes, locked for 16x...
	Locked5x,
	/// 6x votes, locked for 32x...
	Locked6x,
}

impl From<Conviction> for u8 {
	fn from(c: Conviction) -> u8 {
		match c {
			Conviction::None => 0,
			Conviction::Locked1x => 1,
			Conviction::Locked2x => 2,
			Conviction::Locked3x => 3,
			Conviction::Locked4x => 4,
			Conviction::Locked5x => 5,
			Conviction::Locked6x => 6,
		}
	}
}

/// A number of lock periods, plus a vote, one way or the other.
pub struct DemocracyVote {
	pub aye: bool,
	pub conviction: Conviction,
}
impl DemocracyVote {
	fn encode(&self) -> Vote{
        let byte = u8::from(self.conviction) | if self.aye { 0b1000_0000 } else { 0 };
        Vote(byte)
	}
}

pub async fn propose_upgrade(api: &OnlineClient<PolkadotConfig>, acc_seed_accounts : &[sr25519::Pair]) -> Result<(), Box<dyn std::error::Error>> {
    // User 20 will submit the preimage call.
    let i = 20;
    let call = Call::System(SystemCall::set_code {
        code: WASM_BINARY.expect("Could not read the wasm binary.").into()
    }).encode();
    let tx_params = Params::new()
        .tip(PlainTip::new(0))
        .era(Era::Immortal, api.genesis_hash());
    let preimage_hash = BlakeTwo256::hash(&call[..]);
    let submit_preimage_tx = polkadot::tx().democracy().note_preimage(call);
    let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
    let hash = api.tx().sign_and_submit(&submit_preimage_tx, &acc_signer, tx_params).await?;
    println!("Note preimage extrinsic submitted for test account {}: {}",i, hash);
    // User 21 will submit the proposal call.
    let i = 21;
    let value = api.constants().at(&polkadot::constants().democracy().minimum_deposit()).unwrap();
    let tx = polkadot::tx().democracy().propose(
        preimage_hash,
        value,
    );
    let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
    let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
    println!("Democracy proposal extrinsic submitted for test account {}: {}",i, hash);
    Ok(())
}

pub async fn vote(api: &OnlineClient<PolkadotConfig>, acc_seed_accounts : &[sr25519::Pair], ref_index: u32, approve: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();
    let bound = if approve {4*NB_VOTERS/5} else {NB_VOTERS/5};
    let tx_params = Params::new()
        .tip(PlainTip::new(0))
        .era(Era::Immortal, api.genesis_hash());
    let aye = DemocracyVote{ aye: true, conviction: if approve {Conviction::Locked4x} else {Conviction::Locked1x}};
    let aye_v = if approve {TEST_ACCOUNT_FUNDING / 5} else {TEST_ACCOUNT_FUNDING / 2000};
    let nay = DemocracyVote{ aye: false, conviction: if !approve {Conviction::Locked4x} else {Conviction::Locked1x}};
    let nay_v = if approve {TEST_ACCOUNT_FUNDING / 2000} else {TEST_ACCOUNT_FUNDING / 5};
    // Votes with a bias as per function call
    for _ in 0..bound {
        let k: usize = rng.gen_range(0..acc_seed_accounts.len()/2 as usize);
        let v = AccountVote::Standard { vote: aye.encode(), balance: aye_v };
        let tx = polkadot::tx().democracy().vote(ref_index, v);
        let acc_signer = PairSigner::new(acc_seed_accounts[k as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Aye vote extrinsic submitted for test account {:?}: {}",k, hash);
    }
    for _ in bound..NB_VOTERS {
        let k: usize = rng.gen_range(acc_seed_accounts.len()/2 as usize .. acc_seed_accounts.len() as usize);
        let v = AccountVote::Standard { vote: nay.encode(), balance: nay_v };
        let tx = polkadot::tx().democracy().vote(ref_index, v);
        let acc_signer = PairSigner::new(acc_seed_accounts[k as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Nay vote extrinsic submitted for test account {:?}: {}",k, hash);
    }
    tokio::time::sleep(Duration::from_secs(BLOCK_INCLUSION_LAG)).await;
    tokio::time::sleep(Duration::from_secs(2 * 60)).await;
    Ok(())
}