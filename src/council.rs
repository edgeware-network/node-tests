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
        sp_core::{sr25519, Pair},
        sp_runtime::{
            AccountId32,
            traits::{BlakeTwo256, Hash}
        },
    },
    OnlineClient,
    PolkadotConfig,
};
use rand::Rng;
use crate::consts::*;

#[subxt::subxt(runtime_metadata_path = "./data/metadata.scale")]
pub mod polkadot {}

type Call = polkadot::runtime_types::edgeware_runtime::Call;
type DemocracyCall = polkadot::runtime_types::pallet_democracy::pallet::Call;
type TreasuryCall = polkadot::runtime_types::pallet_treasury::pallet::Call;
type RenouncingCandidacy = polkadot::runtime_types::pallet_elections_phragmen::Renouncing;

pub async fn populate_council(api: &OnlineClient<PolkadotConfig>, acc_seed_accounts : &[sr25519::Pair]) -> Result<(), Box<dyn std::error::Error>> {
    let tx_params = Params::new()
        .tip(PlainTip::new(0))
        .era(Era::Immortal, api.genesis_hash());
    // All councillors renounce candidacy
    let tx = polkadot::tx().phragmen_election().renounce_candidacy(RenouncingCandidacy::Member);
    for i in 0..NB_COUNCILLOR_CANDIDATES {
        let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Councillor membership removal extrinsic submitted for test account {}: {}",i, hash);
    }
    // Submit Candidacy to the council
    for i in 0..NB_COUNCILLOR_CANDIDATES {
        let tx = polkadot::tx().phragmen_election().submit_candidacy(i as u32);
        let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Councillor candidacy extrinsic submitted for test account {}: {}",i, hash);
    }
    let mut rng = rand::thread_rng();
    for i in 0..NB_COUNCILLOR_CANDIDATES {
        let n: i32 = rng.gen_range(0..10);
        let mut votes = Vec::new();
        for _ in 0..n {
            let k: u32 = rng.gen_range(0..NB_COUNCILLOR_CANDIDATES);
            votes.push(acc_seed_accounts[k as usize].public().into());
        }
        votes.sort();
        votes.dedup();
        let tx = polkadot::tx().phragmen_election().vote(votes, TEST_ACCOUNT_FUNDING / 10);
        let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Councillor vote extrinsic submitted for test account {}: {}",i, hash);
    }
    // Second candidate renounce candidacy
    let i = 2;
    let tx = polkadot::tx().phragmen_election().renounce_candidacy(RenouncingCandidacy::Candidate(i));
    let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
    // submit the transaction:
    let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
    println!("Councillor candidacy removal extrinsic submitted for test account {}: {}",i, hash);
    // Get the councillors after 6 mins.
    tokio::time::sleep(Duration::from_secs(60 * 6)).await;
    // Drop 3 councillors so that 3 runner ups take the seats.
    for i in 0..3 {
        let tx = polkadot::tx().phragmen_election().renounce_candidacy(RenouncingCandidacy::Member);
        let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
        // submit the transaction:
        let hash = api.tx().sign_and_submit(&tx, &acc_signer, tx_params).await?;
        println!("Councillor membership removal extrinsic submitted for test account {}: {}",i, hash);
    }
    tokio::time::sleep(Duration::from_secs(BLOCK_INCLUSION_LAG)).await;
    Ok(())
}


pub async fn external_majority_workflow(api: &OnlineClient<PolkadotConfig>, acc_seed_accounts : &[sr25519::Pair]) -> Result<(), Box<dyn std::error::Error>> {
    let tx_params = Params::new()
        .tip(PlainTip::new(0))
        .era(Era::Immortal, api.genesis_hash());
    let councillors_addr = polkadot::storage().phragmen_election().members();
    let councillors = api.storage().fetch(&councillors_addr, None).await?.unwrap();
    if 0==councillors.len(){
        panic!("The council has not been setup.");
    }
    // Create proposals
    let acc0id: AccountId32 = acc_seed_accounts[0 as usize].clone().public().into();
    let treasury_proposal_tx = polkadot::tx().treasury().propose_spend(
        TEST_ACCOUNT_FUNDING,
        acc0id.clone().into(),
    );
    let treasury_proposal_storage_index = polkadot::storage().treasury().proposal_count();
    let treasury_proposal_index_before = api.storage().fetch(&treasury_proposal_storage_index, None).await?;
    let i = 10;
    let acc_signer = PairSigner::new(acc_seed_accounts[i as usize].clone());
    // submit the transaction:
    let hash = api.tx().sign_and_submit(&treasury_proposal_tx, &acc_signer, tx_params).await?;
    println!("Treasury proposal extrinsic created for test account {}: {}",i, hash);
    tokio::time::sleep(Duration::from_secs(BLOCK_INCLUSION_LAG)).await;
    let treasury_proposal_index = api.storage().fetch(&treasury_proposal_storage_index, None).await?;
    let treasury_proposal_index = if let Some(t) = treasury_proposal_index {
        if 0 == t {
            panic!("ERROR: Treasury proposal incorrectly registered");
        }
        assert_eq!(if 1 == t {None} else {Some(t-1)}, treasury_proposal_index_before);
        t-1
    } else {
        panic!("ERROR: Treasury proposal incorrectly registered");
    };
    // Noting the preimage by account 10, may or may not be a councillor.
    let call = Call::Treasury(TreasuryCall::approve_proposal { proposal_id: treasury_proposal_index }).encode();
    let preimage_hash = BlakeTwo256::hash(&call[..]);
    let submit_preimage_tx = polkadot::tx().democracy().note_preimage(call);
    let hash = api.tx().sign_and_submit(&submit_preimage_tx, &acc_signer, tx_params).await?;
    println!("Note preimage extrinsic submitted for test account {}: {}",i, hash);
    // Councillor 0 proposes
    let call = Call::Democracy(DemocracyCall::external_propose_majority { proposal_hash: preimage_hash });
    let call_hash = BlakeTwo256::hash(&call.encode()[..]);
    let tx = polkadot::tx().council().propose(8, call, 42);
    let c0_pos = acc_seed_accounts.iter().position(|x|councillors[0].who == x.public().into());
    let c0_signer = PairSigner::new(acc_seed_accounts[c0_pos.unwrap() as usize].clone());
    let hash = api.tx().sign_and_submit(&tx, &c0_signer, tx_params).await?;
    println!("External propose majority extrinsic submitted for councillor {}: {}",0, hash);
    tokio::time::sleep(Duration::from_secs(BLOCK_INCLUSION_LAG)).await;
    // The councillors vote
    let council_proposal_storage_index = polkadot::storage().council().proposal_count();
    let council_proposal_index = api.storage().fetch(&council_proposal_storage_index, None).await?;
    let council_proposal_index = if let Some(t) = council_proposal_index {t-1} else {0};
    let tx = polkadot::tx().council().vote(call_hash, council_proposal_index, true);
    for c in councillors.iter() {
        let c_pos = acc_seed_accounts.iter().position(|x|c.who == x.public().into());
        let c_signer = PairSigner::new(acc_seed_accounts[c_pos.unwrap() as usize].clone());
        let hash = api.tx().sign_and_submit(&tx, &c_signer, tx_params).await?;
        println!("Councillor vote extrinsic submitted for councillor {:?}, account {:?}: {}",c.who, c_pos, hash);
    }
    tokio::time::sleep(Duration::from_secs(BLOCK_INCLUSION_LAG)).await;
    // Councillor 0 closes
    let tx = polkadot::tx().council().close(
        call_hash,
        council_proposal_index,
        WEIGHT_BOUND,
        LENGTH_BOUND);
    let c_pos = acc_seed_accounts.iter().position(|x|councillors[0].who == x.public().into());
    let c_signer = PairSigner::new(acc_seed_accounts[c_pos.unwrap() as usize].clone());
    let hash = api.tx().sign_and_submit(&tx, &c_signer, tx_params).await?;
    println!("Councillor close extrinsic submitted by councillor {:?}, account {:?}: {}",councillors[0].who, c_pos, hash);
    tokio::time::sleep(Duration::from_secs(60+BLOCK_INCLUSION_LAG)).await;
    Ok(())
}