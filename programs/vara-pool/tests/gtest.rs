//! Integration tests for the kVARA pool through the generated client: value moves with
//! the messages, payouts land on user balances, sessions act for their owner.

use ::vara_pool_client::{
    VaraPoolClient as _, VaraPoolClientCtors as _, VaraPoolClientProgram,
    pool::{self, Pool as _, events::PoolEvents},
    vft::{Vft as _, events::VftEvents},
};
use sails_rs::prelude::futures::StreamExt as _;
#[allow(unused_imports)]
use sails_rs::{client::*, gtest::*, prelude::*};

const ALICE: u64 = DEFAULT_USER_ALICE;
const BOB: u64 = DEFAULT_USER_BOB;
const CHARLIE: u64 = DEFAULT_USER_CHARLIE;
const ONE: u128 = 1_000_000_000_000; // 12 decimals
const SCALE: u128 = 1_000_000_000_000_000_000;
const FEE_BPS: u32 = 30;
const UNBOND_SECS: u64 = 60; // 20 blocks
const VESTING_SECS: u64 = 3_600; // the minimum: a tranche vests over 1,200 blocks
const BLOCK_MS: u64 = 3000;

type PoolActor = Actor<VaraPoolClientProgram, GtestEnv>;

fn actor(n: u64) -> ActorId {
    ActorId::from(n)
}
fn u(n: u128) -> U256 {
    U256::from(n)
}
fn user_env(env: &GtestEnv, user: u64) -> GtestEnv {
    env.clone().with_actor_id(actor(user))
}

struct World {
    env: GtestEnv,
    pool: PoolActor,
}

impl World {
    fn pool_as(&self, user: u64) -> PoolActor {
        Actor::new(user_env(&self.env, user), self.pool.id())
    }
    fn vara(&self, who: u64) -> u128 {
        self.env.system().balance_of(who)
    }
    async fn shares(&self, who: ActorId) -> U256 {
        self.pool.vft().balance_of(who).await.unwrap()
    }
    fn skip_secs(&self, secs: u64) {
        let blocks = (secs * 1000 / BLOCK_MS) as u32 + 1;
        self.env.system().run_to_block(self.env.system().block_height() + blocks);
    }
}

/// Deploy the pool as Alice at rate 1.0 with a one minute vesting period.
async fn setup() -> World {
    let env = GtestEnv::system_default();
    for user in [BOB, CHARLIE] {
        env.system().mint_to(user, DEFAULT_USERS_INITIAL_BALANCE);
    }
    let code = env.system().submit_code(::vara_pool::WASM_BINARY);
    let pool = env
        .deploy::<VaraPoolClientProgram>(code, b"kvara".to_vec())
        .new("Vale kVARA".into(), "kVARA".into(), 12, FEE_BPS, UNBOND_SECS, VESTING_SECS, U256::zero())
        .await
        .unwrap();
    // Programs need balance to pay for the messages they send.
    env.system().transfer(ALICE, pool.id(), 100 * ONE, true);
    World { env, pool }
}

#[tokio::test]
async fn constructor_state_and_metadata() {
    let w = setup().await;
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.admin, actor(ALICE));
    assert_eq!(info.symbol, "kVARA");
    assert_eq!(info.decimals, 12);
    assert_eq!(info.rate, U256::from(SCALE));
    assert_eq!(info.apy_bps, 0, "nothing is vesting");
    assert_eq!(info.instant_fee_bps, FEE_BPS);
    assert_eq!(info.unbond_period_secs, UNBOND_SECS);
    assert_eq!(info.vesting_period_secs, VESTING_SECS);
    assert_eq!(info.locked_rewards, U256::zero());
    assert_eq!(info.total_shares, U256::zero());
    assert_eq!(info.reserve, U256::zero());
    assert!(!info.paused);
    assert_eq!(w.pool.vft().symbol().await.unwrap(), "kVARA");
}

#[tokio::test]
async fn stake_moves_value_in_and_mints_one_to_one() {
    let w = setup().await;
    let mut kvft_events = w.pool.vft().listen().await.unwrap();
    let mut pool_events = w.pool.pool().listen().await.unwrap();
    let bob_before = w.vara(BOB);

    let shares = w.pool_as(BOB).pool().stake().with_value(500 * ONE).await.unwrap().unwrap();
    assert_eq!(shares, u(500 * ONE), "rate 1.0 => 1:1");
    assert_eq!(w.shares(actor(BOB)).await, u(500 * ONE));
    assert!(w.vara(BOB) <= bob_before - 500 * ONE, "bob paid the stake");
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.total_shares, u(500 * ONE));
    assert_eq!(info.reserve, u(500 * ONE));

    let (_, ev) = kvft_events.next().await.unwrap();
    assert_eq!(ev, VftEvents::Transfer { from: ActorId::zero(), to: actor(BOB), value: u(500 * ONE) });
    let (_, ev) = pool_events.next().await.unwrap();
    assert!(matches!(ev, PoolEvents::Staked { owner, assets, shares, .. } if owner == actor(BOB) && assets == u(500 * ONE) && shares == u(500 * ONE)), "{ev:?}");
}

#[tokio::test]
async fn stake_below_minimum_is_refunded() {
    let w = setup().await;
    let bob_before = w.vara(BOB);
    let res = w.pool_as(BOB).pool().stake().with_value(1_000).await.unwrap();
    assert!(matches!(res, Err(pool::PoolError::BelowMinimum { .. })), "{res:?}");
    assert_eq!(w.shares(actor(BOB)).await, U256::zero());
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, U256::zero(), "nothing kept");
    // The attached value came back with the reply (only gas was spent).
    assert!(bob_before - w.vara(BOB) < ONE, "refund missing: {} -> {}", bob_before, w.vara(BOB));
}

#[tokio::test]
async fn unstake_pays_net_of_fee_from_the_reserve() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(500 * ONE).await.unwrap().unwrap();
    let mut pool_events = w.pool.pool().listen().await.unwrap();
    let bob_before = w.vara(BOB);

    let out = w.pool_as(BOB).pool().unstake(u(200 * ONE)).await.unwrap().unwrap();
    let fee = 200 * ONE * FEE_BPS as u128 / 10_000;
    assert_eq!(out.assets, u(200 * ONE));
    assert_eq!(out.fee, u(fee));
    assert_eq!(out.net, u(200 * ONE - fee));
    assert_eq!(w.shares(actor(BOB)).await, u(300 * ONE));
    let got = w.vara(BOB) - bob_before;
    assert!(got > 199 * ONE && got <= 200 * ONE - fee, "bob received {got}");
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, u(500 * ONE) - out.net);
    assert_eq!(info.fees_accrued, out.fee);
    let (_, ev) = pool_events.next().await.unwrap();
    assert!(matches!(ev, PoolEvents::Unstaked { owner, shares, .. } if owner == actor(BOB) && shares == u(200 * ONE)), "{ev:?}");

    // More than the balance is refused without side effects.
    let res = w.pool_as(BOB).pool().unstake(u(301 * ONE)).await.unwrap();
    assert_eq!(res, Err(pool::PoolError::InsufficientShares));
    assert_eq!(w.shares(actor(BOB)).await, u(300 * ONE));
}

#[tokio::test]
async fn rewards_vest_linearly_and_are_paid_on_exit() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    let mut events = w.pool.pool().listen().await.unwrap();
    let reserve = w.pool_as(CHARLIE).pool().fund_rewards().with_value(10 * ONE).await.unwrap().unwrap();
    assert_eq!(reserve, u(110 * ONE));
    let (_, ev) = events.next().await.unwrap();
    assert!(matches!(ev, PoolEvents::RewardsFunded { from, assets, .. } if from == actor(CHARLIE) && assets == u(10 * ONE)), "{ev:?}");

    // Funding does not jump the rate: the tranche is locked and releases over the period
    // (one block later, 1/1200 of it is out).
    let info = w.pool.pool().info().await.unwrap();
    assert!(info.rate <= U256::from(SCALE + SCALE / 10_000), "rate barely moved at funding: {}", info.rate);
    assert!(info.locked_rewards > u(9_990 * ONE / 1_000), "locked: {}", info.locked_rewards);
    assert!(info.distributable < u(100_010 * ONE / 1_000), "distributable: {}", info.distributable);
    assert!(info.apy_bps > 0, "the drip is visible as an APY: {}", info.apy_bps);
    assert!(info.vesting_ends_at > 0);

    w.skip_secs(VESTING_SECS / 2);
    let mid = w.pool.pool().rate().await.unwrap();
    assert!(mid > U256::from(SCALE) && mid < U256::from(SCALE + SCALE / 10), "half way: {mid}");
    let owed = w.pool.pool().position(actor(BOB)).await.unwrap().assets;
    assert!(owed > u(100 * ONE) && owed < u(110 * ONE), "position grew: {owed}");

    w.skip_secs(VESTING_SECS);
    let info = w.pool.pool().info().await.unwrap();
    let full = U256::from(SCALE + SCALE / 10);
    assert!(info.rate <= full && info.rate + U256::from(SCALE / 1_000_000) >= full, "fully vested: 1.10 up to the virtual offset, got {}", info.rate);
    assert_eq!(info.locked_rewards, U256::zero());
    assert_eq!((info.apy_bps, info.vesting_ends_at), (0, 0), "no tranche left");

    let bob_before = w.vara(BOB);
    let out = w.pool_as(BOB).pool().unstake(u(100 * ONE)).await.unwrap().unwrap();
    assert!(out.assets <= u(110 * ONE) && out.assets + u(ONE / 1_000) >= u(110 * ONE), "principal plus the whole tranche up to the virtual offset: {}", out.assets);
    assert!(w.vara(BOB) > bob_before + 109 * ONE, "bob got principal plus yield");
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, u(110 * ONE) - out.net);
    assert_eq!(info.total_shares, U256::zero());
    assert_eq!(info.rate, U256::from(SCALE + SCALE / 10), "an empty pool keeps its last rate");
}

#[tokio::test]
async fn instant_round_trip_earns_nothing() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(1_000 * ONE).await.unwrap().unwrap();
    w.pool_as(CHARLIE).pool().fund_rewards().with_value(100 * ONE).await.unwrap().unwrap();
    w.skip_secs(VESTING_SECS / 2);
    let bob_owed = w.pool.pool().position(actor(BOB)).await.unwrap().assets;

    // Charlie visits: in, then straight out at the very next block.
    let charlie_before = w.vara(CHARLIE);
    let shares = w.pool_as(CHARLIE).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    assert!(shares < u(100 * ONE), "priced at the grown rate: {shares}");
    let out = w.pool_as(CHARLIE).pool().unstake(shares).await.unwrap().unwrap();
    assert!(out.assets < u(100 * ONE) + u(ONE / 100), "one block of the pro-rata drip at most: {}", out.assets);
    assert!(out.net < u(100 * ONE), "the fee makes it a loss: {}", out.net);
    assert!(w.vara(CHARLIE) < charlie_before, "charlie is down gas and fee");
    assert!(w.pool.pool().position(actor(BOB)).await.unwrap().assets >= bob_owed, "bob lost nothing to the visit");
}

#[tokio::test]
async fn unbond_then_claim_after_period() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(300 * ONE).await.unwrap().unwrap();
    let entry = w.pool_as(BOB).pool().request_unbond(u(100 * ONE)).await.unwrap().unwrap();
    assert_eq!(entry.assets, u(100 * ONE));
    assert_eq!(w.shares(actor(BOB)).await, u(200 * ONE));
    let listed = w.pool.pool().unbonds(actor(BOB)).await.unwrap();
    assert_eq!(listed, vec![entry.clone()]);

    let early = w.pool_as(BOB).pool().claim(entry.id).await.unwrap();
    assert!(matches!(early, Err(pool::PoolError::UnbondNotReady { .. })), "{early:?}");
    let wrong_owner = w.pool_as(CHARLIE).pool().claim(entry.id).await.unwrap();
    assert_eq!(wrong_owner, Err(pool::PoolError::UnbondNotFound));

    w.skip_secs(UNBOND_SECS);
    let bob_before = w.vara(BOB);
    let paid = w.pool_as(BOB).pool().claim(entry.id).await.unwrap().unwrap();
    assert_eq!(paid, u(100 * ONE));
    assert!(w.vara(BOB) > bob_before + 99 * ONE, "claim paid out");
    assert!(w.pool.pool().unbonds(actor(BOB)).await.unwrap().is_empty());
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, u(200 * ONE));
    assert_eq!(info.unbonding_total, U256::zero());
    let again = w.pool_as(BOB).pool().claim(entry.id).await.unwrap();
    assert_eq!(again, Err(pool::PoolError::UnbondNotFound));
}

#[tokio::test]
async fn receipt_token_is_transferable_and_unstakeable_by_holder() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    assert!(w.pool_as(BOB).vft().transfer(actor(CHARLIE), u(40 * ONE)).await.unwrap().unwrap());
    assert_eq!(w.shares(actor(CHARLIE)).await, u(40 * ONE));
    let charlie_before = w.vara(CHARLIE);
    let out = w.pool_as(CHARLIE).pool().unstake(u(40 * ONE)).await.unwrap().unwrap();
    assert!(w.vara(CHARLIE) > charlie_before + 39 * ONE, "charlie was paid {}", out.net);
    assert_eq!(w.shares(actor(CHARLIE)).await, U256::zero());
}

#[tokio::test]
async fn admin_controls_and_fee_collection() {
    let w = setup().await;
    assert_eq!(w.pool_as(BOB).pool().pause().await.unwrap(), Err(pool::PoolError::Unauthorized));
    w.pool.pool().pause().await.unwrap().unwrap();
    let res = w.pool_as(BOB).pool().stake().with_value(10 * ONE).await.unwrap();
    assert_eq!(res, Err(pool::PoolError::Paused));
    w.pool.pool().resume().await.unwrap().unwrap();
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    let out = w.pool_as(BOB).pool().unstake(u(100 * ONE)).await.unwrap().unwrap();
    let charlie_before = w.vara(CHARLIE);
    let collected = w.pool.pool().collect_fees(actor(CHARLIE)).await.unwrap().unwrap();
    assert_eq!(collected, out.fee);
    assert!(w.vara(CHARLIE) > charlie_before, "fees landed");
    assert_eq!(w.pool.pool().collect_fees(actor(CHARLIE)).await.unwrap(), Err(pool::PoolError::ZeroAmount));
    assert_eq!(w.pool.pool().set_config(1_001, UNBOND_SECS, VESTING_SECS).await.unwrap(), Err(pool::PoolError::BadConfig));
    assert_eq!(w.pool.pool().set_config(FEE_BPS, UNBOND_SECS, 60).await.unwrap(), Err(pool::PoolError::BadConfig));
    w.pool.pool().set_config(50, 30, 7_200).await.unwrap().unwrap();
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!((info.instant_fee_bps, info.unbond_period_secs, info.vesting_period_secs), (50, 30, 7_200));
    w.pool.pool().transfer_admin(actor(BOB)).await.unwrap().unwrap();
    assert_eq!(w.pool.pool().pause().await.unwrap(), Err(pool::PoolError::Unauthorized));
}

#[tokio::test]
async fn session_key_acts_for_its_owner_once_it_accepts() {
    let w = setup().await;
    let key = 42u64;
    w.env.system().mint_to(key, DEFAULT_USERS_INITIAL_BALANCE);
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    let actions = vec![pool::SessionAction::Unstake, pool::SessionAction::Unbond, pool::SessionAction::Claim];
    let s = w.pool_as(BOB).pool().create_session(actor(key), 3_600, actions).await.unwrap().unwrap();
    assert_eq!(s.key, actor(key));
    // Proposed only: the key is still a plain account with no shares.
    assert_eq!(w.pool.pool().session_owner(actor(key)).await.unwrap(), None);
    assert_eq!(w.pool.pool().session(actor(BOB)).await.unwrap(), None);
    assert_eq!(w.pool.pool().pending_session(actor(BOB)).await.unwrap(), Some(s.clone()));
    assert_eq!(w.pool_as(key).pool().unstake(u(50 * ONE)).await.unwrap(), Err(pool::PoolError::InsufficientShares));

    let accepted = w.pool_as(key).pool().accept_session(actor(BOB)).await.unwrap().unwrap();
    assert_eq!(accepted, s);
    assert_eq!(w.pool.pool().session_owner(actor(key)).await.unwrap(), Some(actor(BOB)));
    assert_eq!(w.pool.pool().pending_session(actor(BOB)).await.unwrap(), None);

    // The key unstakes for Bob: Bob's shares burn and Bob (not the key) is paid.
    let bob_before = w.vara(BOB);
    let key_before = w.vara(key);
    let out = w.pool_as(key).pool().unstake(u(50 * ONE)).await.unwrap().unwrap();
    assert_eq!(w.shares(actor(BOB)).await, u(50 * ONE));
    assert!(w.vara(BOB) > bob_before + 49 * ONE, "owner paid {}", out.net);
    assert!(w.vara(key) < key_before, "the key paid gas only");
    // Staking through the key is not in the allow list.
    let res = w.pool_as(key).pool().stake().with_value(10 * ONE).await.unwrap();
    assert_eq!(res, Err(pool::PoolError::SessionNotAllowed));
    assert!(w.pool_as(BOB).pool().revoke_session().await.unwrap());
    assert_eq!(w.pool.pool().session(actor(BOB)).await.unwrap(), None);
    assert_eq!(w.pool.pool().session_owner(actor(key)).await.unwrap(), None);
}

#[tokio::test]
async fn proposing_a_stranger_as_key_cannot_capture_their_stake() {
    // Regression: before sessions needed acceptance, anyone could name another account as
    // "their key" and that account's own stake would mint to the attacker.
    let w = setup().await;
    let all = vec![pool::SessionAction::Stake, pool::SessionAction::Unstake, pool::SessionAction::Unbond, pool::SessionAction::Claim];
    w.pool_as(CHARLIE).pool().create_session(actor(BOB), 3_600, all).await.unwrap().unwrap();
    let shares = w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    assert_eq!(w.shares(actor(BOB)).await, shares, "bob's stake is bob's");
    assert_eq!(w.shares(actor(CHARLIE)).await, U256::zero());
    // Bob can still exit as himself.
    let out = w.pool_as(BOB).pool().unstake(shares).await.unwrap().unwrap();
    assert_eq!(out.assets, u(100 * ONE));
    // And the stranger accepting nothing means the proposal stays inert; Charlie can drop it.
    assert!(w.pool_as(CHARLIE).pool().revoke_session().await.unwrap());
}

#[tokio::test]
async fn value_attached_to_a_plain_command_comes_back_and_surplus_can_be_rescued() {
    let w = setup().await;
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    // A command that takes no value returns whatever came with it.
    let bob_before = w.vara(BOB);
    let had = w.pool_as(BOB).pool().revoke_session().with_value(5 * ONE).await.unwrap();
    assert!(!had);
    assert!(bob_before - w.vara(BOB) < ONE, "the 5 VARA came back, only gas was spent: {}", bob_before - w.vara(BOB));
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, u(100 * ONE), "nothing leaked into the reserve");

    // VARA that lands on the program outside its books (a plain transfer here) is admin-rescuable,
    // but never the reserve or the keep-alive buffer.
    w.env.system().transfer(ALICE, w.pool.id(), 7 * ONE, true);
    let too_much = w.pool.pool().rescue_surplus(actor(CHARLIE), u(200 * ONE)).await.unwrap();
    assert!(matches!(too_much, Err(pool::PoolError::InsufficientReserve { .. })), "{too_much:?}");
    assert_eq!(w.pool_as(BOB).pool().rescue_surplus(actor(BOB), u(ONE)).await.unwrap(), Err(pool::PoolError::Unauthorized));
    let charlie_before = w.vara(CHARLIE);
    let got = w.pool.pool().rescue_surplus(actor(CHARLIE), u(7 * ONE)).await.unwrap().unwrap();
    assert_eq!(got, u(7 * ONE));
    assert!(w.vara(CHARLIE) > charlie_before + 6 * ONE, "rescued VARA landed");
    assert_eq!(w.pool.pool().info().await.unwrap().reserve, u(100 * ONE), "the reserve is untouched");
    // Bob's position is intact and still pays in full.
    let out = w.pool_as(BOB).pool().unstake(u(100 * ONE)).await.unwrap().unwrap();
    assert_eq!(out.assets, u(100 * ONE));
}

#[tokio::test]
async fn pool_can_start_at_a_higher_rate() {
    let env = GtestEnv::system_default();
    env.system().mint_to(BOB, DEFAULT_USERS_INITIAL_BALANCE);
    let code = env.system().submit_code(::vara_pool::WASM_BINARY);
    let start = U256::from(SCALE + SCALE / 20); // 1.05
    let pool = env
        .deploy::<VaraPoolClientProgram>(code, b"kvara-105".to_vec())
        .new("Vale kVARA".into(), "kVARA".into(), 12, FEE_BPS, UNBOND_SECS, VESTING_SECS, start)
        .await
        .unwrap();
    env.system().transfer(ALICE, pool.id(), 100 * ONE, true);
    assert_eq!(pool.pool().rate().await.unwrap(), start);
    let bob = Actor::new(user_env(&env, BOB), pool.id());
    let shares = bob.pool().stake().with_value(105 * ONE).await.unwrap().unwrap();
    assert_eq!(shares, u(100 * ONE), "105 VARA buys 100 kVARA at 1.05");
    assert_eq!(pool.pool().position(actor(BOB)).await.unwrap().assets, u(105 * ONE));
}
