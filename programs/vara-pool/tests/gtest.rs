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
const APY_BPS: u32 = 100_000; // 1000% so accrual is visible within a few thousand blocks
const FEE_BPS: u32 = 30;
const UNBOND_SECS: u64 = 60; // 20 blocks
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

/// Deploy the pool as Alice with a zero APY (share math stays exact); accrual tests switch it on.
async fn setup() -> World {
    let env = GtestEnv::system_default();
    for user in [BOB, CHARLIE] {
        env.system().mint_to(user, DEFAULT_USERS_INITIAL_BALANCE);
    }
    let code = env.system().submit_code(::vara_pool::WASM_BINARY);
    let pool = env
        .deploy::<VaraPoolClientProgram>(code, b"kvara".to_vec())
        .new("Vale kVARA".into(), "kVARA".into(), 12, 0, FEE_BPS, UNBOND_SECS)
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
    assert_eq!(info.apy_bps, 0);
    assert_eq!(info.instant_fee_bps, FEE_BPS);
    assert_eq!(info.unbond_period_secs, UNBOND_SECS);
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
async fn rate_accrues_and_yield_is_paid_from_funded_rewards() {
    let w = setup().await;
    // Stake at exactly 1.0, then switch the APY on (and the fee off, so the payout exceeds the
    // principal) while the share count stays a round 100.
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    w.pool.pool().set_config(APY_BPS, 0, UNBOND_SECS).await.unwrap().unwrap();
    w.skip_secs(3_600); // 1000% APY over an hour ≈ +0.114%
    let rate = w.pool.pool().rate().await.unwrap();
    assert!(rate > U256::from(SCALE), "rate moved: {rate}");
    let owed = w.pool.pool().position(actor(BOB)).await.unwrap().assets;
    assert!(owed > u(100 * ONE), "position grew: {owed}");

    // Only the principal is in the reserve: paying out everything needs rewards first.
    let res = w.pool_as(BOB).pool().unstake(u(100 * ONE)).await.unwrap();
    assert!(matches!(res, Err(pool::PoolError::InsufficientReserve { .. })), "{res:?}");
    assert_eq!(w.shares(actor(BOB)).await, u(100 * ONE), "position intact");

    let reserve = w.pool_as(CHARLIE).pool().fund_rewards().with_value(10 * ONE).await.unwrap().unwrap();
    assert_eq!(reserve, u(110 * ONE));
    let bob_before = w.vara(BOB);
    let out = w.pool_as(BOB).pool().unstake(u(100 * ONE)).await.unwrap().unwrap();
    assert!(out.assets > u(100 * ONE), "yield paid: {}", out.assets);
    assert!(w.vara(BOB) > bob_before + 100 * ONE - ONE, "bob got principal plus yield");
    let info = w.pool.pool().info().await.unwrap();
    assert_eq!(info.reserve, u(110 * ONE) - out.net);
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
    assert_eq!(w.pool.pool().set_config(MAX_APY_BPS_PLUS_ONE, FEE_BPS, UNBOND_SECS).await.unwrap(), Err(pool::PoolError::BadConfig));
    w.pool.pool().transfer_admin(actor(BOB)).await.unwrap().unwrap();
    assert_eq!(w.pool.pool().pause().await.unwrap(), Err(pool::PoolError::Unauthorized));
}
const MAX_APY_BPS_PLUS_ONE: u32 = 100_001;

#[tokio::test]
async fn session_key_acts_for_its_owner() {
    let w = setup().await;
    let key = 42u64;
    w.env.system().mint_to(key, DEFAULT_USERS_INITIAL_BALANCE);
    w.pool_as(BOB).pool().stake().with_value(100 * ONE).await.unwrap().unwrap();
    let actions = vec![pool::SessionAction::Unstake, pool::SessionAction::Unbond, pool::SessionAction::Claim];
    let s = w.pool_as(BOB).pool().create_session(actor(key), 3_600, actions).await.unwrap().unwrap();
    assert_eq!(s.key, actor(key));
    assert_eq!(w.pool.pool().session_owner(actor(key)).await.unwrap(), Some(actor(BOB)));

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
}
