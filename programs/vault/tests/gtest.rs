//! Integration tests: a demo token and a vault deployed together, driven through
//! the generated clients, including the cross-program calls the vault makes.
//!
//! Build order: `cargo build --release` in `programs/demo-token` first, this test
//! embeds its optimised wasm.

use ::vault_app::token_client::{
    DemoToken as _, DemoTokenCtors as _, DemoTokenProgram, admin::Admin as _, faucet::Faucet as _, vft::Vft as _,
};
use ::vault_client::{
    VaultClient as _, VaultClientCtors as _, VaultClientProgram,
    vault::{self, Vault as _, events::VaultEvents},
    vft::{self as kvft, Vft as _, events::VftEvents},
};
use sails_rs::prelude::futures::StreamExt as _;
#[allow(unused_imports)]
use sails_rs::{client::*, gtest::*, prelude::*};

const TOKEN_WASM: &[u8] = include_bytes!("../../demo-token/target/wasm32-gear/release/demo_token.opt.wasm");

const ALICE: u64 = DEFAULT_USER_ALICE;
const BOB: u64 = DEFAULT_USER_BOB;
const CHARLIE: u64 = DEFAULT_USER_CHARLIE;
const ONE: u64 = 1_000_000; // 6 decimals
const FAUCET: u64 = 1_000 * ONE;
const SCALE: u128 = 1_000_000_000_000_000_000;
const APY_BPS: u32 = 100_000; // 1000% so accrual is visible within a few thousand blocks
const FEE_BPS: u32 = 30;
const UNBOND_SECS: u64 = 60; // 20 blocks
const BLOCK_MS: u64 = 3000;

type Token = Actor<DemoTokenProgram, GtestEnv>;
type Vault = Actor<VaultClientProgram, GtestEnv>;

fn actor(n: u64) -> ActorId {
    ActorId::from(n)
}
fn u(n: u64) -> U256 {
    U256::from(n)
}
fn user_env(env: &GtestEnv, user: u64) -> GtestEnv {
    env.clone().with_actor_id(actor(user))
}

struct World {
    env: GtestEnv,
    token: Token,
    vault: Vault,
}

impl World {
    fn token_as(&self, user: u64) -> Token {
        Actor::new(user_env(&self.env, user), self.token.id())
    }
    fn vault_as(&self, user: u64) -> Vault {
        Actor::new(user_env(&self.env, user), self.vault.id())
    }
    async fn token_balance(&self, who: ActorId) -> U256 {
        self.token.vft().balance_of(who).await.unwrap()
    }
    async fn shares(&self, who: ActorId) -> U256 {
        self.vault.vft().balance_of(who).await.unwrap()
    }
    fn skip_secs(&self, secs: u64) {
        let blocks = (secs * 1000 / BLOCK_MS) as u32 + 1;
        self.env.system().run_to_block(self.env.system().block_height() + blocks);
    }
}

/// Deploy token + vault as Alice, grant the vault minter, give Bob faucet balance and approval.
async fn setup() -> World {
    let env = GtestEnv::system_default();
    for user in [BOB, CHARLIE] {
        env.system().mint_to(user, DEFAULT_USERS_INITIAL_BALANCE);
    }
    let token_code = env.system().submit_code(TOKEN_WASM);
    let token = env
        .deploy::<DemoTokenProgram>(token_code, b"usdc".to_vec())
        .new("Vale Demo USDC".into(), "USDC".into(), 6, u(FAUCET), 60)
        .await
        .unwrap();
    let vault_code = env.system().submit_code(::vault::WASM_BINARY);
    let vault = env
        .deploy::<VaultClientProgram>(vault_code, b"kusdc".to_vec())
        // APY starts at zero so share math in the tests is exact; accrual tests switch it on.
        .new(token.id(), "Vale kUSDC".into(), "kUSDC".into(), 6, 0, FEE_BPS, UNBOND_SECS)
        .await
        .unwrap();
    // Programs need balance to pay for the messages they send.
    env.system().transfer(ALICE, vault.id(), 10_000 * 1_000_000_000_000u128, true);
    token.admin().grant_minter(vault.id()).await.unwrap().unwrap();

    let w = World { env, token, vault };
    w.token_as(BOB).faucet().claim().await.unwrap().unwrap();
    w.token_as(BOB).vft().approve(w.vault.id(), U256::MAX).await.unwrap().unwrap();
    w
}

#[tokio::test]
async fn constructor_state_and_metadata() {
    let w = setup().await;
    let info = w.vault.vault().info().await.unwrap();
    assert_eq!(info.underlying, w.token.id());
    assert_eq!(info.admin, actor(ALICE));
    assert_eq!(info.symbol, "kUSDC");
    assert_eq!(info.decimals, 6);
    assert_eq!(info.rate, U256::from(SCALE));
    assert_eq!(info.apy_bps, 0);
    assert_eq!(info.instant_fee_bps, FEE_BPS);
    assert_eq!(info.unbond_period_secs, UNBOND_SECS);
    assert_eq!(info.total_shares, U256::zero());
    assert!(!info.paused);
    assert_eq!(w.vault.vft().symbol().await.unwrap(), "kUSDC");
    assert_eq!(w.vault.vft().total_supply().await.unwrap(), U256::zero());
    assert!(w.token.admin().is_minter(w.vault.id()).await.unwrap());
}

#[tokio::test]
async fn deposit_is_one_to_one_and_moves_underlying() {
    let w = setup().await;
    let mut kvft_events = w.vault.vft().listen().await.unwrap();
    let mut vault_events = w.vault.vault().listen().await.unwrap();

    let shares = w.vault_as(BOB).vault().deposit(u(500 * ONE)).await.unwrap().unwrap();
    assert_eq!(shares, u(500 * ONE), "rate 1.0 => 1:1");
    assert_eq!(w.shares(actor(BOB)).await, u(500 * ONE));
    assert_eq!(w.token_balance(actor(BOB)).await, u(500 * ONE));
    assert_eq!(w.token_balance(w.vault.id()).await, u(500 * ONE));
    let info = w.vault.vault().info().await.unwrap();
    assert_eq!(info.total_shares, u(500 * ONE));
    assert_eq!(info.holdings, u(500 * ONE));

    let (_, ev) = kvft_events.next().await.unwrap();
    assert_eq!(ev, VftEvents::Transfer { from: ActorId::zero(), to: actor(BOB), value: u(500 * ONE) });
    let (_, ev) = vault_events.next().await.unwrap();
    assert!(matches!(ev, VaultEvents::Deposited { owner, assets, shares, .. } if owner == actor(BOB) && assets == u(500 * ONE) && shares == u(500 * ONE)), "{ev:?}");

    // Second deposit while the rate is still 1.0 within the same second is also 1:1.
    let more = w.vault_as(BOB).vault().deposit(u(100 * ONE)).await.unwrap().unwrap();
    assert!(more <= u(100 * ONE) && more > u(99 * ONE), "{more}");
}

#[tokio::test]
async fn deposit_without_allowance_fails_without_side_effects() {
    let w = setup().await;
    // Charlie has faucet balance but never approved the vault.
    w.token_as(CHARLIE).faucet().claim().await.unwrap().unwrap();
    let res = w.vault_as(CHARLIE).vault().deposit(u(10 * ONE)).await.unwrap();
    assert!(matches!(&res, Err(vault::VaultError::TokenRejected(m)) if m.contains("InsufficientAllowance")), "{res:?}");
    assert_eq!(w.shares(actor(CHARLIE)).await, U256::zero());
    assert_eq!(w.token_balance(actor(CHARLIE)).await, u(FAUCET));
    assert_eq!(w.vault.vault().info().await.unwrap().holdings, U256::zero());

    // Validation errors before any transfer.
    let small = w.vault_as(BOB).vault().deposit(u(999)).await.unwrap();
    assert_eq!(small, Err(vault::VaultError::BelowMinimum { min: u(1_000) }));
    let zero = w.vault_as(BOB).vault().deposit(U256::zero()).await.unwrap();
    assert_eq!(zero, Err(vault::VaultError::ZeroAmount));
}

#[tokio::test]
async fn rate_accrues_and_redeem_pays_yield_by_minting_shortfall() {
    let w = setup().await;
    w.vault_as(BOB).vault().deposit(u(500 * ONE)).await.unwrap().unwrap();
    w.vault.vault().set_config(APY_BPS, FEE_BPS, UNBOND_SECS).await.unwrap().unwrap();
    let supply_before = w.token.vft().total_supply().await.unwrap();

    w.skip_secs(30_000); // ~0.95% at 1000% APY
    let rate = w.vault.vault().rate().await.unwrap();
    assert!(rate > U256::from(SCALE), "rate moved: {rate}");
    let pos = w.vault.vault().position(actor(BOB)).await.unwrap();
    assert_eq!(pos.shares, u(500 * ONE));
    assert!(pos.assets > u(500 * ONE) && pos.assets < u(510 * ONE), "{pos:?}");

    let preview = w.vault.vault().preview_redeem(u(500 * ONE)).await.unwrap();
    let bob_before = w.token_balance(actor(BOB)).await;
    let out = w.vault_as(BOB).vault().redeem(u(500 * ONE)).await.unwrap().unwrap();
    assert!(out.assets >= preview.assets, "{out:?} vs {preview:?}");
    assert_eq!(out.fee, out.assets * U256::from(FEE_BPS) / U256::from(10_000u64));
    assert_eq!(out.net, out.assets - out.fee);
    assert!(out.net > u(500 * ONE), "yield paid");

    assert_eq!(w.token_balance(actor(BOB)).await, bob_before + out.net);
    assert_eq!(w.shares(actor(BOB)).await, U256::zero());
    let info = w.vault.vault().info().await.unwrap();
    assert_eq!(info.total_shares, U256::zero());
    assert_eq!(info.fees_accrued, out.fee);
    assert_eq!(info.holdings, U256::zero(), "all principal paid out, yield was minted lazily");
    let supply_after = w.token.vft().total_supply().await.unwrap();
    assert_eq!(supply_after - supply_before, out.net - u(500 * ONE), "exactly the realised yield was minted");
}

#[tokio::test]
async fn unbond_then_claim_after_period() {
    let w = setup().await;
    w.vault_as(BOB).vault().deposit(u(300 * ONE)).await.unwrap().unwrap();
    let entry = w.vault_as(BOB).vault().request_unbond(u(100 * ONE)).await.unwrap().unwrap();
    assert_eq!(entry.shares, u(100 * ONE));
    assert!(entry.assets >= u(100 * ONE));
    assert_eq!(entry.claimable_at, entry.requested_at + UNBOND_SECS * 1000);
    assert_eq!(w.shares(actor(BOB)).await, u(200 * ONE));
    assert_eq!(w.vault.vault().unbonds(actor(BOB)).await.unwrap(), vec![entry.clone()]);
    assert_eq!(w.vault.vault().info().await.unwrap().unbonding_total, entry.assets);

    let early = w.vault_as(BOB).vault().claim(entry.id).await.unwrap();
    assert_eq!(early, Err(vault::VaultError::UnbondNotReady { claimable_at: entry.claimable_at }));
    let wrong_owner = w.vault_as(CHARLIE).vault().claim(entry.id).await.unwrap();
    assert_eq!(wrong_owner, Err(vault::VaultError::UnbondNotFound));

    w.skip_secs(UNBOND_SECS);
    let bob_before = w.token_balance(actor(BOB)).await;
    let paid = w.vault_as(BOB).vault().claim(entry.id).await.unwrap().unwrap();
    assert_eq!(paid, entry.assets);
    assert_eq!(w.token_balance(actor(BOB)).await, bob_before + paid);
    assert!(w.vault.vault().unbonds(actor(BOB)).await.unwrap().is_empty());
    assert_eq!(w.vault.vault().info().await.unwrap().unbonding_total, U256::zero());
    let twice = w.vault_as(BOB).vault().claim(entry.id).await.unwrap();
    assert_eq!(twice, Err(vault::VaultError::UnbondNotFound));
}

#[tokio::test]
async fn receipt_token_is_transferable_and_redeemable_by_holder() {
    let w = setup().await;
    w.vault_as(BOB).vault().deposit(u(200 * ONE)).await.unwrap().unwrap();
    assert!(w.vault_as(BOB).vft().transfer(actor(CHARLIE), u(50 * ONE)).await.unwrap().unwrap());
    assert_eq!(w.shares(actor(CHARLIE)).await, u(50 * ONE));
    let over = w.vault_as(BOB).vft().transfer(actor(CHARLIE), u(151 * ONE)).await.unwrap();
    assert_eq!(over, Err(kvft::VaultError::InsufficientBalance));

    let out = w.vault_as(CHARLIE).vault().redeem(u(50 * ONE)).await.unwrap().unwrap();
    assert_eq!(w.token_balance(actor(CHARLIE)).await, out.net);
    assert_eq!(w.shares(actor(CHARLIE)).await, U256::zero());
    assert_eq!(w.vault.vft().total_supply().await.unwrap(), u(150 * ONE));
}

#[tokio::test]
async fn admin_controls() {
    let w = setup().await;
    let denied = w.vault_as(BOB).vault().set_config(100, 10, 5).await.unwrap();
    assert_eq!(denied, Err(vault::VaultError::Unauthorized));
    let bad = w.vault.vault().set_config(200_000, 10, 5).await.unwrap();
    assert_eq!(bad, Err(vault::VaultError::BadConfig));
    w.vault.vault().set_config(500, 50, 10).await.unwrap().unwrap();
    let info = w.vault.vault().info().await.unwrap();
    assert_eq!((info.apy_bps, info.instant_fee_bps, info.unbond_period_secs), (500, 50, 10));

    w.vault.vault().pause().await.unwrap().unwrap();
    let paused = w.vault_as(BOB).vault().deposit(u(10 * ONE)).await.unwrap();
    assert_eq!(paused, Err(vault::VaultError::Paused));
    w.vault.vault().resume().await.unwrap().unwrap();
    w.vault_as(BOB).vault().deposit(u(100 * ONE)).await.unwrap().unwrap();

    // Fees: redeem everything at 0.5% then collect to Alice (vault mints the shortfall for fees too).
    let all = w.shares(actor(BOB)).await;
    let out = w.vault_as(BOB).vault().redeem(all).await.unwrap().unwrap();
    assert!(out.fee > U256::zero());
    let denied = w.vault_as(BOB).vault().collect_fees(actor(BOB)).await.unwrap();
    assert_eq!(denied, Err(vault::VaultError::Unauthorized));
    let alice_before = w.token_balance(actor(ALICE)).await;
    let collected = w.vault.vault().collect_fees(actor(ALICE)).await.unwrap().unwrap();
    assert_eq!(collected, out.fee);
    assert_eq!(w.token_balance(actor(ALICE)).await, alice_before + collected);
    assert_eq!(w.vault.vault().info().await.unwrap().fees_accrued, U256::zero());

    // Reserve top-up mints into the vault.
    w.vault.vault().top_up_reserve(u(1_000 * ONE)).await.unwrap().unwrap();
    assert_eq!(w.vault.vault().info().await.unwrap().holdings, u(1_000 * ONE));
    assert_eq!(w.token_balance(w.vault.id()).await, u(1_000 * ONE));

    w.vault.vault().transfer_admin(actor(BOB)).await.unwrap().unwrap();
    assert_eq!(w.vault.vault().info().await.unwrap().admin, actor(BOB));
}

#[tokio::test]
async fn accrue_checkpoints_and_emits() {
    let w = setup().await;
    w.vault_as(BOB).vault().deposit(u(100 * ONE)).await.unwrap().unwrap();
    w.vault.vault().set_config(APY_BPS, FEE_BPS, UNBOND_SECS).await.unwrap().unwrap();
    let mut events = w.vault.vault().listen().await.unwrap();
    w.skip_secs(3_000);
    let rate = w.vault_as(CHARLIE).vault().accrue().await.unwrap();
    assert!(rate > U256::from(SCALE));
    let (_, ev) = events.next().await.unwrap();
    assert!(matches!(ev, VaultEvents::Accrued { rate: r, .. } if r == rate), "{ev:?}");
    let info = w.vault.vault().info().await.unwrap();
    assert!(info.rate >= rate, "checkpoint persisted and keeps projecting: {} >= {rate}", info.rate);
    assert!(info.last_accrual_at > info.last_accrual_at - 3_000 * 1_000, "clock advanced");
}
