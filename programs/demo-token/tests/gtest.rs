use ::demo_token_client::{DemoTokenClient as _, DemoTokenClientCtors as _, DemoTokenClientProgram, admin, admin::Admin as _, faucet, faucet::Faucet as _, vft::Vft as _, vft::TokenError, vft::events::VftEvents};
use sails_rs::prelude::futures::StreamExt as _;
#[allow(unused_imports)]
use sails_rs::{client::*, gtest::*, prelude::*};

const ALICE: u64 = DEFAULT_USER_ALICE;
const BOB: u64 = DEFAULT_USER_BOB;
const CHARLIE: u64 = DEFAULT_USER_CHARLIE;
const ONE: u64 = 1_000_000; // 6 decimals
const FAUCET: u64 = 1_000 * ONE;
const COOLDOWN_SECS: u64 = 60; // 20 blocks at 3s
const BLOCK_MS: u64 = 3000;

fn actor(n: u64) -> ActorId {
    ActorId::from(n)
}

type Token = Actor<DemoTokenClientProgram, GtestEnv>;

/// The same program seen through another signer.
fn as_user(env: &GtestEnv, program: &Token, user: u64) -> Token {
    Actor::new(env.clone().with_actor_id(actor(user)), program.id())
}

async fn deploy(env: &GtestEnv) -> Token {
    let code_id = env.system().submit_code(::demo_token::WASM_BINARY);
    env.deploy::<DemoTokenClientProgram>(code_id, b"salt".to_vec())
        .new("Vale Demo USDC".into(), "USDC".into(), 6, U256::from(FAUCET), COOLDOWN_SECS)
        .await
        .unwrap()
}

#[tokio::test]
async fn metadata_and_admin_are_set_by_constructor() {
    let env = GtestEnv::system_default();
    let program = deploy(&env).await;
    let vft = program.vft();
    assert_eq!(vft.name().await.unwrap(), "Vale Demo USDC");
    assert_eq!(vft.symbol().await.unwrap(), "USDC");
    assert_eq!(vft.decimals().await.unwrap(), 6);
    assert_eq!(vft.total_supply().await.unwrap(), U256::zero());
    assert_eq!(program.admin().admin_account().await.unwrap(), actor(ALICE));
    let cfg = program.faucet().config().await.unwrap();
    assert_eq!(cfg.amount, U256::from(FAUCET));
    assert_eq!(cfg.cooldown_secs, COOLDOWN_SECS);
}

#[tokio::test]
async fn faucet_mints_and_enforces_cooldown() {
    let env = GtestEnv::system_default();
    env.system().mint_to(BOB, DEFAULT_USERS_INITIAL_BALANCE);
    let program = deploy(&env).await;
    let bob_program = as_user(&env, &program, BOB);

    let mut vft_events = program.vft().listen().await.unwrap();
    let mut faucet = bob_program.faucet();
    let minted = faucet.claim().await.unwrap().unwrap();
    assert_eq!(minted, U256::from(FAUCET));
    assert_eq!(program.vft().balance_of(actor(BOB)).await.unwrap(), U256::from(FAUCET));
    assert_eq!(program.vft().total_supply().await.unwrap(), U256::from(FAUCET));

    // VFT Transfer event from the zero address is emitted for the mint.
    let (_, ev) = vft_events.next().await.unwrap();
    assert_eq!(ev, VftEvents::Transfer { from: ActorId::zero(), to: actor(BOB), value: U256::from(FAUCET) });

    // A second claim inside the cooldown is a fatal error reply.
    let again = faucet.claim().await.unwrap();
    assert!(matches!(again, Err(faucet::TokenError::FaucetCooldown { .. })), "got {again:?}");
    let next = program.faucet().next_claim_at(actor(BOB)).await.unwrap();
    assert!(next > env.system().block_timestamp());

    // After the cooldown it works again.
    let blocks = (COOLDOWN_SECS * 1000 / BLOCK_MS) as u32 + 1;
    env.system().run_to_block(env.system().block_height() + blocks);
    faucet.claim().await.unwrap().unwrap();
    assert_eq!(program.vft().balance_of(actor(BOB)).await.unwrap(), U256::from(2 * FAUCET));
}

#[tokio::test]
async fn transfer_approve_transfer_from() {
    let env = GtestEnv::system_default();
    env.system().mint_to(BOB, DEFAULT_USERS_INITIAL_BALANCE);
    env.system().mint_to(CHARLIE, DEFAULT_USERS_INITIAL_BALANCE);
    let program = deploy(&env).await;
    program.admin().mint(actor(BOB), U256::from(100 * ONE)).await.unwrap().unwrap();

    let bob_program = as_user(&env, &program, BOB);
    let charlie_program = as_user(&env, &program, CHARLIE);
    let mut bob_vft = bob_program.vft();
    assert!(bob_vft.transfer(actor(CHARLIE), U256::from(40 * ONE)).await.unwrap().unwrap());
    assert_eq!(program.vft().balance_of(actor(BOB)).await.unwrap(), U256::from(60 * ONE));
    assert_eq!(program.vft().balance_of(actor(CHARLIE)).await.unwrap(), U256::from(40 * ONE));

    let too_much = bob_vft.transfer(actor(CHARLIE), U256::from(61 * ONE)).await.unwrap();
    assert_eq!(too_much, Err(TokenError::InsufficientBalance));

    assert!(bob_vft.approve(actor(CHARLIE), U256::from(25 * ONE)).await.unwrap().unwrap());
    assert_eq!(program.vft().allowance(actor(BOB), actor(CHARLIE)).await.unwrap(), U256::from(25 * ONE));

    let mut charlie_vft = charlie_program.vft();
    assert!(charlie_vft.transfer_from(actor(BOB), actor(CHARLIE), U256::from(20 * ONE)).await.unwrap().unwrap());
    assert_eq!(program.vft().allowance(actor(BOB), actor(CHARLIE)).await.unwrap(), U256::from(5 * ONE));
    let over = charlie_vft.transfer_from(actor(BOB), actor(CHARLIE), U256::from(6 * ONE)).await.unwrap();
    assert_eq!(over, Err(TokenError::InsufficientAllowance));
    assert_eq!(program.vft().balance_of(actor(BOB)).await.unwrap(), U256::from(40 * ONE));
    assert_eq!(program.vft().total_supply().await.unwrap(), U256::from(100 * ONE));
}

#[tokio::test]
async fn minter_role_and_pause() {
    let env = GtestEnv::system_default();
    env.system().mint_to(BOB, DEFAULT_USERS_INITIAL_BALANCE);
    let program = deploy(&env).await;
    let bob_program = as_user(&env, &program, BOB);
    let mut bob_admin = bob_program.admin();

    let denied = bob_admin.mint(actor(BOB), U256::from(ONE)).await.unwrap();
    assert_eq!(denied, Err(admin::TokenError::Unauthorized));

    program.admin().grant_minter(actor(BOB)).await.unwrap().unwrap();
    assert!(program.admin().is_minter(actor(BOB)).await.unwrap());
    assert_eq!(program.admin().minters().await.unwrap(), vec![actor(BOB)]);
    bob_admin.mint(actor(BOB), U256::from(ONE)).await.unwrap().unwrap();
    assert_eq!(program.vft().balance_of(actor(BOB)).await.unwrap(), U256::from(ONE));

    program.admin().pause().await.unwrap().unwrap();
    assert!(program.admin().is_paused().await.unwrap());
    let paused = bob_admin.mint(actor(BOB), U256::from(ONE)).await.unwrap();
    assert_eq!(paused, Err(admin::TokenError::Paused));
    program.admin().resume().await.unwrap().unwrap();
    bob_admin.mint(actor(BOB), U256::from(ONE)).await.unwrap().unwrap();

    program.admin().revoke_minter(actor(BOB)).await.unwrap().unwrap();
    let revoked = bob_admin.mint(actor(BOB), U256::from(ONE)).await.unwrap();
    assert_eq!(revoked, Err(admin::TokenError::Unauthorized));

    // Faucet reconfiguration by admin only.
    let denied = bob_admin.set_faucet(U256::from(ONE), 1).await.unwrap();
    assert_eq!(denied, Err(admin::TokenError::Unauthorized));
    program.admin().set_faucet(U256::zero(), 1).await.unwrap().unwrap();
    let disabled = bob_program.faucet().claim().await.unwrap();
    assert_eq!(disabled, Err(faucet::TokenError::FaucetDisabled));
}
