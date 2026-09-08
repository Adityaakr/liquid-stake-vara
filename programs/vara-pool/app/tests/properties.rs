//! Property tests for the host-testable `PoolState` machine.
//!
//! A random sequence of operations (2-4 actors, VARA amounts of 1 VARA .. 10M VARA, up to
//! 400 days of block time) is driven through `PoolState`, and after every step the whole
//! invariant set below is checked. The pool is synchronous on chain, so a failed payout is
//! reverted inside the same message: reverts here are immediate too.
//!
//! Quotes use the virtual offset: `(distributable + VA) / (total_shares + VS)` with
//! `VS = VIRTUAL_SHARES` and `VA = ceil(VS * base_rate / SCALE)`, and an empty pool prices
//! nothing but the virtual pair. Consequences used below: `rate_at(now) >= base_rate` always;
//! burning the last share re-quotes at `ceil`, so the rate may tick up by at most
//! `SCALE / VS` there; and holders' claims sum to at most `distributable` (the virtual shares'
//! slice stays behind), so conservation of value is an inequality, not an equality.
//!
//! Properties (numbered as in the request):
//!  1. Solvency: `reserve >= fees_accrued + unbonding_total + locked_rewards(now)`,
//!     `distributable(now) <= reserve`, and `reserve` equals the VARA that physically entered
//!     the program minus what left it (an external value ledger kept by the harness).
//!  2. Conservation: `total_shares == sum(balances)`, no zero balance is stored,
//!     `unbonding_total == sum(unbond.assets)`, unbond ids are unique, ascending and below
//!     `next_unbond_id`, and `sum(assets_for(balance)) <= distributable` (nothing over-promised).
//!  3. The rate never decreases: not over time, not across any operation. Exact statement per
//!     operation: `stake`, `unstake`, `request_unbond` may raise it by rounding (in the pool's
//!     favour), and by at most `SCALE / VS` when they burn the last share; time may raise it
//!     (vesting); everything else leaves it untouched (`fund` moves nothing at the funding
//!     instant). `rate_at(now) >= base_rate` always.
//!  4. Round trip never profits: stake A then unstake (or unbond) all the minted shares in the
//!     same instant returns <= A and leaves `distributable` and the rate at least where they
//!     were; on an empty pool the residual left to nobody is at most the virtual slice.
//!  5. `assets_for(shares_for(A)) <= A` and `shares_for(assets_for(s)) <= s`.
//!  6. `locked_rewards` is non-increasing in time, bounded by `vest_amount`, and zero at and
//!     after `vest_end_ms`.
//!  7. `fund` releases nothing at the funding instant (`locked` grows by exactly the amount),
//!     never moves `vest_end_ms` past `now + vesting_period_ms` unless the running tranche
//!     already ended later (possible after `set_config` shrank the period), and a 1-unit
//!     fund never moves the end of a running tranche whose locked amount is at least
//!     `period - time_left` (base units vs ms; below that the stretch is bounded by
//!     `(period - time_left) / (locked + 1)` ms, so the literal "never" does not hold).
//!  8. `request_unbond` is rate neutral: reserve unchanged, distributable down by exactly the
//!     assets locked, rate up by at most one rounding tick, other holders' claims untouched.
//!  9. `revert_unstake` / `restore_unbond` / the fee-collection revert restore the exact prior
//!     state, compared field by field. Known exception, allowed explicitly: an unstake that
//!     burned the last share leaves `base_rate` refreshed to the pre-unstake rate (visible only
//!     as an upward nudge of at most `SCALE / VS` on the rate).
//! 10. Sessions: `sessions` and `session_keys` are mutual inverses, a key serves one owner,
//!     and `actor_for` routes a key to its owner only while the session is live and allows the
//!     action, and routes everybody else (expired keys included) to themselves.
//! Plus: every `Err` leaves the state untouched, every `Err` is justified by its documented
//! precondition, and no operation dilutes a holder who did not take part in it.

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use sails_rs::collections::{BTreeMap, BTreeSet};
use sails_rs::prelude::*;
use std::cell::RefCell;
use vara_pool_app::*;

const T0: u64 = 1_700_000_000_000;
const DAY_MS: u64 = 86_400_000;
const HORIZON_MS: u64 = 400 * DAY_MS;
/// Actors 1..=OWNERS hold positions; session keys are drawn from 1..=KEYS so a key can be
/// another owner (allowed) or the owner itself (rejected).
const OWNERS: u64 = 4;
const KEYS: u64 = 8;

/// `prop_assert_eq!` builds its message with `concat!`, which cannot capture `{var}` from the
/// scope; this keeps the left/right diff and lets messages use captures.
macro_rules! check_eq {
    ($l:expr, $r:expr, $($fmt:tt)+) => {{
        let (l, r) = (&$l, &$r);
        prop_assert!(l == r, "{} (left: {:?}, right: {:?})", format!($($fmt)+), l, r);
    }};
}

fn a(n: u64) -> ActorId { ActorId::from(n) }
fn u(n: u128) -> U256 { U256::from(n) }
fn pct_of(v: U256, pct: u8) -> U256 { v * U256::from(pct) / U256::from(100u64) }
/// The most the rate may tick up when the last share is burned and the pool re-quotes at
/// `ceil(VS * rate / SCALE)`.
fn empty_tick() -> U256 { SCALE / VIRTUAL_SHARES + U256::one() }

// ---------------------------------------------------------------------------
// Snapshot: every field of `PoolState`, so a revert can be checked exactly and a state cloned.
// A struct literal on purpose: a field added to `PoolState` fails this build instead of
// silently escaping the comparison.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snap {
    admin: ActorId,
    name: String,
    symbol: String,
    decimals: u8,
    total_shares: U256,
    balances: BTreeMap<ActorId, U256>,
    allowances: BTreeMap<(ActorId, ActorId), U256>,
    base_rate: U256,
    instant_fee_bps: u32,
    unbond_period_ms: u64,
    vesting_period_ms: u64,
    reserve: U256,
    fees_accrued: U256,
    unbonding_total: U256,
    vest_amount: U256,
    vest_start_ms: u64,
    vest_end_ms: u64,
    paused: bool,
    unbonds: BTreeMap<ActorId, Vec<Unbond>>,
    next_unbond_id: u64,
    sessions: BTreeMap<ActorId, Session>,
    session_keys: BTreeMap<ActorId, ActorId>,
    pending_sessions: BTreeMap<ActorId, Session>,
}

fn snap(s: &PoolState) -> Snap {
    Snap {
        admin: s.admin,
        name: s.name.clone(),
        symbol: s.symbol.clone(),
        decimals: s.decimals,
        total_shares: s.total_shares,
        balances: s.balances.clone(),
        allowances: s.allowances.clone(),
        base_rate: s.base_rate,
        instant_fee_bps: s.instant_fee_bps,
        unbond_period_ms: s.unbond_period_ms,
        vesting_period_ms: s.vesting_period_ms,
        reserve: s.reserve,
        fees_accrued: s.fees_accrued,
        unbonding_total: s.unbonding_total,
        vest_amount: s.vest_amount,
        vest_start_ms: s.vest_start_ms,
        vest_end_ms: s.vest_end_ms,
        paused: s.paused,
        unbonds: s.unbonds.clone(),
        next_unbond_id: s.next_unbond_id,
        sessions: s.sessions.clone(),
        session_keys: s.session_keys.clone(),
        pending_sessions: s.pending_sessions.clone(),
    }
}

fn restore(sn: &Snap) -> PoolState {
    PoolState {
        admin: sn.admin,
        name: sn.name.clone(),
        symbol: sn.symbol.clone(),
        decimals: sn.decimals,
        total_shares: sn.total_shares,
        balances: sn.balances.clone(),
        allowances: sn.allowances.clone(),
        base_rate: sn.base_rate,
        instant_fee_bps: sn.instant_fee_bps,
        unbond_period_ms: sn.unbond_period_ms,
        vesting_period_ms: sn.vesting_period_ms,
        reserve: sn.reserve,
        fees_accrued: sn.fees_accrued,
        unbonding_total: sn.unbonding_total,
        vest_amount: sn.vest_amount,
        vest_start_ms: sn.vest_start_ms,
        vest_end_ms: sn.vest_end_ms,
        paused: sn.paused,
        unbonds: sn.unbonds.clone(),
        next_unbond_id: sn.next_unbond_id,
        sessions: sn.sessions.clone(),
        session_keys: sn.session_keys.clone(),
        pending_sessions: sn.pending_sessions.clone(),
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Log-uniform over 10^lo ..= 10^(hi+1): every magnitude gets equal weight, and shrinking
/// walks the exponent down first.
fn log_uniform(lo: u32, hi: u32) -> impl Strategy<Value = u128> {
    (lo..=hi).prop_flat_map(|e| {
        let base = 10u128.pow(e);
        base..base * 10
    })
}

/// A stake: 1 VARA .. 10M VARA, with the minimum, just below it and just above it pinned (the
/// payout floor makes the planck right above the minimum an edge).
fn stake_amount() -> impl Strategy<Value = u128> {
    prop_oneof![7 => log_uniform(12, 24), 1 => Just(MIN_STAKE), 1 => Just(MIN_STAKE - 1), 1 => MIN_STAKE + 1..=MIN_STAKE + 100]
}

/// A reward: realistic tranches plus small ones and 1-planck dust (for the vesting properties).
fn reward_amount() -> impl Strategy<Value = u128> {
    prop_oneof![7 => log_uniform(10, 24), 2 => log_uniform(0, 9), 1 => Just(1)]
}

/// Time between operations: same block, a block, within a day, weeks.
fn dt() -> impl Strategy<Value = u64> {
    prop_oneof![2 => Just(0u64), 3 => 1_000u64..=3_000, 3 => 3_000u64..=DAY_MS, 3 => DAY_MS..=40 * DAY_MS]
}

/// Share of a balance: partial, all, and occasionally none or more than held.
fn pct() -> impl Strategy<Value = u8> {
    prop_oneof![6 => 1u8..=99, 3 => Just(100u8), 1 => 0u8..=120]
}

fn owner() -> impl Strategy<Value = u64> { 1..=OWNERS }
fn key() -> impl Strategy<Value = u64> { 1..=KEYS }

fn actions() -> impl Strategy<Value = Vec<SessionAction>> {
    prop::sample::subsequence(vec![SessionAction::Stake, SessionAction::Unstake, SessionAction::Unbond, SessionAction::Claim], 1..=4)
}

fn duration() -> impl Strategy<Value = u64> {
    prop_oneof![4 => 1u64..=MAX_SESSION_SECS, 1 => Just(0u64), 1 => Just(MAX_SESSION_SECS + 1)]
}

#[derive(Debug, Clone)]
struct Cfg {
    fee_bps: u32,
    unbond_secs: u64,
    vesting_secs: u64,
    initial_rate: u128,
}

/// Out-of-range fee and unbond period included: the constructor clamps them.
fn cfg() -> impl Strategy<Value = Cfg> {
    (
        prop_oneof![3 => 0u32..=MAX_FEE_BPS + 500, 1 => Just(0u32), 1 => Just(30u32)],
        prop_oneof![3 => 0u64..=MAX_UNBOND_SECS + 30 * 86_400, 1 => Just(0u64), 1 => Just(7 * 86_400)],
        prop_oneof![3 => MIN_VESTING_SECS..=MAX_VESTING_SECS, 1 => Just(7 * 86_400)],
        prop_oneof![2 => 1_000_000_000_000_000_000u128..=2_000_000_000_000_000_000u128, 1 => Just(0u128)],
    )
        .prop_map(|(fee_bps, unbond_secs, vesting_secs, initial_rate)| Cfg { fee_bps, unbond_secs, vesting_secs, initial_rate })
}

#[derive(Debug, Clone)]
enum Op {
    Stake { who: u64, amount: u128 },
    Unstake { who: u64, pct: u8, payout_fails: bool },
    Fund { amount: u128 },
    RequestUnbond { who: u64, pct: u8 },
    /// `pick` selects one of the actors with unbonds and one of their entries; a slice of the
    /// range picks a random actor instead, to reach the not-found paths.
    Claim { pick: u8, payout_fails: bool },
    CollectFees { payout_fails: bool },
    Advance { dt_ms: u64 },
    Transfer { from: u64, to: u64, pct: u8 },
    ProposeSession { owner: u64, key: u64, duration_secs: u64, actions: Vec<SessionAction> },
    /// `pick` selects one of the pending proposals; a slice of the range picks a random
    /// (key, owner) pair instead, to reach the rejection paths.
    AcceptSession { pick: u8 },
    RevokeSession { owner: u64 },
    SetConfig { fee_bps: u32, unbond_secs: u64, vesting_secs: u64 },
    SetPaused(bool),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        5 => (owner(), stake_amount()).prop_map(|(who, amount)| Op::Stake { who, amount }),
        4 => (owner(), pct(), any::<bool>()).prop_map(|(who, pct, payout_fails)| Op::Unstake { who, pct, payout_fails }),
        3 => reward_amount().prop_map(|amount| Op::Fund { amount }),
        3 => (owner(), pct()).prop_map(|(who, pct)| Op::RequestUnbond { who, pct }),
        3 => (any::<u8>(), any::<bool>()).prop_map(|(pick, payout_fails)| Op::Claim { pick, payout_fails }),
        1 => any::<bool>().prop_map(|payout_fails| Op::CollectFees { payout_fails }),
        6 => dt().prop_map(|dt_ms| Op::Advance { dt_ms }),
        2 => (owner(), owner(), pct()).prop_map(|(from, to, pct)| Op::Transfer { from, to, pct }),
        2 => (owner(), key(), duration(), actions()).prop_map(|(owner, key, duration_secs, actions)| Op::ProposeSession { owner, key, duration_secs, actions }),
        2 => any::<u8>().prop_map(|pick| Op::AcceptSession { pick }),
        1 => owner().prop_map(|owner| Op::RevokeSession { owner }),
        1 => (0u32..=MAX_FEE_BPS + 10, 0u64..=MAX_UNBOND_SECS + 1, MIN_VESTING_SECS - 1..=MAX_VESTING_SECS + 1)
            .prop_map(|(fee_bps, unbond_secs, vesting_secs)| Op::SetConfig { fee_bps, unbond_secs, vesting_secs }),
        1 => any::<bool>().prop_map(Op::SetPaused),
    ]
}

/// One step: an operation plus a probe amount used for the per-step conversion and
/// round-trip checks (properties 4 and 5) on a copy of the state.
#[derive(Debug, Clone)]
struct Step {
    op: Op,
    probe: u128,
}

fn step() -> impl Strategy<Value = Step> {
    (op(), stake_amount()).prop_map(|(op, probe)| Step { op, probe })
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// How the rate is allowed to move across one step.
#[derive(Debug, Clone, Copy)]
enum RateRule {
    Same,
    Up,
    /// The last share was burned: the pool re-quotes at `ceil(VS * rate / SCALE)`, up by at
    /// most `SCALE / VS`.
    Empty,
    /// `request_unbond` with `left` shares remaining: up by at most one rounding tick.
    Unbond { left: U256 },
    /// An unstake that burned the last share and was reverted: the shares are back but
    /// `base_rate` was refreshed to the pre-unstake rate, which raises the virtual assets by
    /// `VS * (rate - old_base) / SCALE` (+1 for the ceil) and the quote by their share of it.
    Revert { old_base: U256 },
}

struct Done {
    label: &'static str,
    /// The state was meant to change. When false the step must have left it byte-identical.
    changed: bool,
    rate: RateRule,
    /// Holders whose claim legitimately moved; everybody else must not be diluted.
    actors: Vec<ActorId>,
}

struct World {
    s: PoolState,
    now: u64,
    /// VARA that entered the program (stakes, rewards) and left it (payouts, fees).
    value_in: U256,
    value_out: U256,
    hits: BTreeMap<&'static str, u32>,
}

type Check = Result<(), TestCaseError>;

fn claims(s: &PoolState, now: u64) -> BTreeMap<ActorId, U256> {
    s.balances.iter().map(|(who, bal)| (*who, s.assets_for(*bal, now).unwrap_or(U256::MAX))).collect()
}

/// True when `key` is held by a live session of an owner other than `owner`.
fn bound_elsewhere(s: &PoolState, key: ActorId, owner: ActorId, now: u64) -> bool {
    s.session_keys.get(&key).is_some_and(|other| *other != owner && s.sessions.get(other).is_some_and(|x| now < x.expires_at))
}

fn session_invariants(s: &PoolState, now: u64, ctx: &str) -> Check {
    // The maps stay mutual inverses for every stored session, live or expired: a dead binding
    // is only ever removed together with its session.
    check_eq!(s.sessions.len(), s.session_keys.len(), "{ctx}: sessions and session_keys differ in size");
    for (owner, sess) in &s.sessions {
        prop_assert!(!sess.key.is_zero() && sess.key != *owner && !sess.actions.is_empty(), "{ctx}: malformed session {sess:?} for {owner:?}");
        check_eq!(s.session_keys.get(&sess.key), Some(owner), "{ctx}: session key {:?} of {owner:?} does not map back", sess.key);
    }
    for (key, owner) in &s.session_keys {
        check_eq!(s.sessions.get(owner).map(|x| x.key), Some(*key), "{ctx}: session_keys[{key:?}] = {owner:?} has no matching session");
    }
    for (owner, p) in &s.pending_sessions {
        prop_assert!(!p.key.is_zero() && p.key != *owner && !p.actions.is_empty(), "{ctx}: malformed proposal {p:?} for {owner:?}");
    }
    let all = [SessionAction::Stake, SessionAction::Unstake, SessionAction::Unbond, SessionAction::Claim];
    for id in 1..=KEYS {
        let sender = a(id);
        let live = s.session_keys.get(&sender).map(|owner| (*owner, &s.sessions[owner])).filter(|(_, x)| now < x.expires_at);
        for act in all {
            let expect = match live {
                None => Ok(sender),
                Some((owner, x)) if x.actions.contains(&act) => Ok(owner),
                Some(_) => Err(PoolError::SessionNotAllowed),
            };
            check_eq!(s.actor_for(sender, act, now), expect, "{ctx}: routing of {sender:?} for {act:?}");
        }
    }
    Ok(())
}

/// Properties 1, 2, 3 (base rate), 6 and 10 on the current state.
fn invariants(w: &World, ctx: &str) -> Check {
    let s = &w.s;
    let now = w.now;
    let locked = s.locked_rewards(now);
    let owed = s.fees_accrued.checked_add(s.unbonding_total).and_then(|x| x.checked_add(locked)).expect("owed overflow");
    prop_assert!(s.reserve >= owed, "{ctx}: insolvent: reserve {} < fees {} + unbonding {} + locked {}", s.reserve, s.fees_accrued, s.unbonding_total, locked);
    prop_assert!(s.distributable(now) <= s.reserve, "{ctx}: distributable exceeds the reserve");
    check_eq!(s.reserve, w.value_in - w.value_out, "{ctx}: reserve does not match the value that entered minus what left");

    let sum = s.balances.values().try_fold(U256::zero(), |acc, b| acc.checked_add(*b)).expect("balance sum overflow");
    check_eq!(s.total_shares, sum, "{ctx}: total_shares != sum of balances");
    prop_assert!(s.balances.values().all(|b| !b.is_zero()), "{ctx}: a zero balance is stored");

    let mut ids = BTreeSet::new();
    let mut unbonding = U256::zero();
    for (owner, list) in &s.unbonds {
        prop_assert!(!list.is_empty(), "{ctx}: empty unbond list kept for {owner:?}");
        for pair in list.windows(2) {
            prop_assert!(pair[0].id < pair[1].id, "{ctx}: unbonds of {owner:?} not ascending");
        }
        for e in list {
            prop_assert!(e.id < s.next_unbond_id && ids.insert(e.id), "{ctx}: unbond id {} duplicated or beyond next_unbond_id", e.id);
            prop_assert!(e.claimable_at >= e.requested_at && e.assets >= u(MIN_PAYOUT) && !e.shares.is_zero(), "{ctx}: malformed unbond {e:?}");
            unbonding += e.assets;
        }
    }
    check_eq!(s.unbonding_total, unbonding, "{ctx}: unbonding_total != sum of entries");

    let d = s.distributable(now);
    if !s.total_shares.is_zero() {
        let promised = claims(s, now).values().fold(U256::zero(), |acc, c| acc.saturating_add(*c));
        prop_assert!(promised <= d, "{ctx}: holders are promised {promised} but only {d} is distributable");
    }
    prop_assert!(s.rate_at(now) >= s.base_rate, "{ctx}: rate {} fell below the base rate {}", s.rate_at(now), s.base_rate);
    prop_assert!(s.base_rate >= SCALE, "{ctx}: base rate {} below 1.0", s.base_rate);

    prop_assert!(locked <= s.vest_amount, "{ctx}: locked {locked} > vest_amount {}", s.vest_amount);
    prop_assert!(s.vest_end_ms >= s.vest_start_ms, "{ctx}: vesting window inverted");
    prop_assert!(s.locked_rewards(s.vest_end_ms).is_zero() && s.locked_rewards(s.vest_end_ms + 1).is_zero(), "{ctx}: rewards still locked at vest_end");
    let mut prev = locked;
    for delta in [1u64, 1_000, DAY_MS, 30 * DAY_MS, HORIZON_MS] {
        let later = s.locked_rewards(now + delta);
        prop_assert!(later <= prev, "{ctx}: locked_rewards rose from {prev} to {later} after +{delta}ms");
        prev = later;
    }

    session_invariants(s, now, ctx)
}

/// Properties 4 and 5 on a copy of the state, with the step's probe amount. Returns a coverage
/// label for the rare paths it reached.
fn probes(w: &World, probe: u128, ctx: &str) -> Result<Option<&'static str>, TestCaseError> {
    let s = &w.s;
    let now = w.now;
    let amount = u(probe);
    let sh = s.shares_for(amount, now).expect("shares_for overflow");
    prop_assert!(s.assets_for(sh, now).expect("assets_for overflow") <= amount, "{ctx}: assets_for(shares_for({amount})) > {amount}");
    for bal in s.balances.values() {
        let back = s.shares_for(s.assets_for(*bal, now).expect("assets_for overflow"), now).expect("shares_for overflow");
        prop_assert!(back <= *bal, "{ctx}: shares_for(assets_for({bal})) = {back} > {bal}");
    }

    if s.paused || probe < MIN_STAKE {
        return Ok(None);
    }
    let who = a(OWNERS + 1); // a visitor with no position
    let d_before = s.distributable(now);
    let rate_before = s.rate_at(now);
    let shares_before = s.total_shares;
    let mut c = restore(&snap(s));
    let minted = c.stake(who, amount, now);
    if sh.is_zero() {
        // The rate has outgrown the minimum stake (rewards dwarf the principal): the stake
        // would mint nothing and must be refused rather than swallowed.
        check_eq!(minted, Err(PoolError::BelowMinimum { min: u(MIN_STAKE) }), "{ctx}: a zero-share stake of {amount} was not refused");
        return Ok(Some("probe: zero-share stake refused"));
    }
    check_eq!(minted, Ok(sh), "{ctx}: probe stake of {amount} minted other than quoted");
    let staked = snap(&c);
    let preview = c.preview_unstake(sh, now).expect("preview overflow");
    let out = match c.unstake(who, sh, now) {
        Ok(out) => out,
        Err(PoolError::BelowMinimum { min }) => {
            // The payout floor: the net of a fresh stake near the minimum can be below 1 VARA.
            prop_assert!(min == u(MIN_PAYOUT) && preview.net < u(MIN_PAYOUT), "{ctx}: unstake of a fresh stake of {amount} refused with {min} although net is {}", preview.net);
            // The fee-free exit must still be open, except within rounding of the minimum:
            // an unbond quote loses at most one share's price plus a planck to rounding.
            let mut c2 = restore(&staked);
            let tick = c2.rate_at(now) / SCALE + U256::from(3u64);
            match c2.request_unbond(who, sh, now) {
                Ok(e) => {
                    prop_assert!(e.assets <= amount, "{ctx}: unbond of a fresh stake of {amount} quotes {}", e.assets);
                    return Ok(Some("probe: unstake below payout floor, unbond ok"));
                }
                Err(PoolError::BelowMinimum { min }) => {
                    prop_assert!(min == u(MIN_PAYOUT) && amount < u(MIN_STAKE) + tick, "{ctx}: a fresh stake of {amount} can be neither unstaked (net {}) nor unbonded (assets {})", preview.net, c2.assets_for(sh, now).unwrap());
                    return Ok(Some("probe: fresh minimum stake cannot exit"));
                }
                Err(e) => prop_assert!(false, "{ctx}: unbond of a fresh stake refused: {e:?}"),
            }
            unreachable!()
        }
        Err(e) => {
            prop_assert!(false, "{ctx}: fresh stake of {amount} ({sh} shares) cannot be unstaked: {e:?}");
            unreachable!()
        }
    };
    prop_assert!(out.assets <= amount, "{ctx}: round trip of {amount} returned {} gross", out.assets);
    prop_assert!(out.net + out.fee == out.assets && out.fee <= out.assets / U256::from(10u64), "{ctx}: fee split wrong: {out:?}");
    if shares_before.is_zero() {
        // An empty pool: whatever had vested with nobody holding is orphaned and the first
        // stake sweeps it into fees; the round trip may leave behind at most the virtual
        // shares' slice of the stake, which the next stake sweeps in turn.
        let residual = c.distributable(now);
        let slice = amount * VIRTUAL_SHARES / (VIRTUAL_SHARES + sh) + U256::one();
        prop_assert!(residual <= slice, "{ctx}: round trip on an empty pool left {residual} distributable, more than the virtual slice {slice}");
        check_eq!(c.fees_accrued, s.fees_accrued + d_before + out.fee, "{ctx}: orphaned {d_before} did not land in fees");
    } else {
        prop_assert!(c.distributable(now) >= d_before, "{ctx}: round trip lowered distributable from {d_before} to {}", c.distributable(now));
    }
    prop_assert!(c.rate_at(now) >= rate_before, "{ctx}: round trip lowered the rate from {rate_before} to {}", c.rate_at(now));
    check_eq!(c.total_shares, shares_before, "{ctx}: round trip changed total shares");
    prop_assert!(c.balance_of(&who).is_zero(), "{ctx}: visitor kept shares after a full round trip");
    Ok(None)
}

fn hit(w: &mut World, label: &'static str) {
    *w.hits.entry(label).or_default() += 1;
}

/// Apply one operation, checking its own contract (post-conditions on success, a justified
/// error on failure). Generic properties are checked afterwards by `run`.
fn apply(w: &mut World, op: &Op, before: &Snap, ctx: &str) -> Result<Done, TestCaseError> {
    let now = w.now;
    let none = Vec::new();
    Ok(match op {
        Op::Stake { who, amount } => {
            let who = a(*who);
            let amount = u(*amount);
            let quote = w.s.shares_for(amount, now).expect("shares_for overflow");
            let bal = w.s.balance_of(&who);
            match w.s.stake(who, amount, now) {
                Ok(shares) => {
                    hit(w, "stake");
                    w.value_in += amount;
                    prop_assert!(!shares.is_zero() && shares == quote, "{ctx}: minted {shares}, quoted {quote}");
                    check_eq!(w.s.balance_of(&who), bal + shares, "{ctx}: balance after stake");
                    check_eq!(w.s.total_shares, before.total_shares + shares, "{ctx}: supply after stake");
                    check_eq!(w.s.reserve, before.reserve + amount, "{ctx}: reserve after stake");
                    if !before.total_shares.is_zero() {
                        check_eq!(w.s.fees_accrued, before.fees_accrued, "{ctx}: stake into a live pool touched fees");
                    }
                    Done { label: "stake", changed: true, rate: RateRule::Up, actors: vec![who] }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::ZeroAmount => amount.is_zero(),
                        PoolError::BelowMinimum { min } => *min == u(MIN_STAKE) && (amount < u(MIN_STAKE) || quote.is_zero()),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: stake of {amount} refused without cause: {e:?}");
                    Done { label: "stake/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::Unstake { who, pct, payout_fails } => {
            let who = a(*who);
            let bal = w.s.balance_of(&who);
            let shares = pct_of(bal, *pct);
            let d = w.s.distributable(now);
            let quote = if shares <= w.s.total_shares { w.s.preview_unstake(shares, now).ok() } else { None };
            match w.s.unstake(who, shares, now) {
                Ok(p) => {
                    check_eq!(Some(&p), quote.as_ref(), "{ctx}: unstake paid other than the preview");
                    prop_assert!(p.assets <= d, "{ctx}: unstake paid {} out of {d} distributable", p.assets);
                    prop_assert!(p.net + p.fee == p.assets && p.net >= u(MIN_PAYOUT), "{ctx}: bad split {p:?}");
                    check_eq!(p.fee, p.assets * U256::from(w.s.instant_fee_bps) / U256::from(BPS), "{ctx}: fee is not instant_fee_bps of assets");
                    check_eq!(w.s.balance_of(&who), bal - shares, "{ctx}: balance after unstake");
                    check_eq!(w.s.total_shares, before.total_shares - shares, "{ctx}: supply after unstake");
                    check_eq!(w.s.reserve, before.reserve - p.net, "{ctx}: reserve after unstake");
                    check_eq!(w.s.fees_accrued, before.fees_accrued + p.fee, "{ctx}: fees after unstake");
                    check_eq!(w.s.distributable(now), d - p.assets, "{ctx}: distributable after unstake");
                    let emptied = w.s.total_shares.is_zero();
                    if *payout_fails {
                        w.s.revert_unstake(who, shares, &p);
                        hit(w, "unstake+revert");
                        Done { label: "unstake+revert", changed: false, rate: if emptied { RateRule::Revert { old_base: before.base_rate } } else { RateRule::Same }, actors: none }
                    } else {
                        hit(w, "unstake");
                        w.value_out += p.net;
                        Done { label: "unstake", changed: true, rate: if emptied { RateRule::Empty } else { RateRule::Up }, actors: vec![who] }
                    }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::ZeroAmount => shares.is_zero(),
                        PoolError::InsufficientShares => shares > bal,
                        PoolError::BelowMinimum { min } => *min == u(MIN_PAYOUT) && quote.as_ref().is_some_and(|q| q.net < u(MIN_PAYOUT)),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: unstake of {shares} (balance {bal}, preview {quote:?}) refused without cause: {e:?}");
                    Done { label: "unstake/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::Fund { amount } => {
            let amount = u(*amount);
            let locked_before = w.s.locked_rewards(now);
            let end_before = w.s.vest_end_ms;
            let period = w.s.vesting_period_ms;
            let d = w.s.distributable(now);
            match w.s.fund(amount, now) {
                Ok(()) => {
                    hit(w, "fund");
                    w.value_in += amount;
                    check_eq!(w.s.reserve, before.reserve + amount, "{ctx}: reserve after fund");
                    check_eq!(w.s.locked_rewards(now), locked_before + amount, "{ctx}: fund released something at the funding instant");
                    check_eq!(w.s.distributable(now), d, "{ctx}: fund moved distributable");
                    prop_assert!(w.s.vest_end_ms > now, "{ctx}: tranche ends in the past");
                    prop_assert!(w.s.vest_end_ms <= end_before.max(now + period), "{ctx}: fund pushed vest_end to {} past max(prior end {end_before}, now+period {})", w.s.vest_end_ms, now + period);
                    if end_before <= now + period {
                        prop_assert!(w.s.vest_end_ms <= now + period, "{ctx}: fund pushed vest_end to {} past now+period {}", w.s.vest_end_ms, now + period);
                    }
                    // Dust cannot stretch a running tranche whose locked amount (base units) is
                    // at least the time it has left to a full period (ms): below that the
                    // amount-weighted average moves the end by (period - left) / (locked + 1) ms,
                    // which the literal claim "dust never stretches" does not cover (see report).
                    let left = end_before.saturating_sub(now);
                    if amount == U256::one() && !locked_before.is_zero() && locked_before >= U256::from(period.saturating_sub(left)) {
                        prop_assert!(w.s.vest_end_ms <= end_before, "{ctx}: 1-planck fund stretched a running tranche from {end_before} to {} (locked {locked_before}, period {period}ms)", w.s.vest_end_ms);
                    }
                    Done { label: "fund", changed: true, rate: RateRule::Same, actors: none }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::ZeroAmount => amount.is_zero(),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: fund of {amount} refused without cause: {e:?}");
                    Done { label: "fund/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::RequestUnbond { who, pct } => {
            let who = a(*who);
            let bal = w.s.balance_of(&who);
            let shares = pct_of(bal, *pct);
            let d = w.s.distributable(now);
            let quote = if shares <= w.s.total_shares { w.s.assets_for(shares, now).ok() } else { None };
            match w.s.request_unbond(who, shares, now) {
                Ok(e) => {
                    hit(w, "request_unbond");
                    prop_assert!(Some(e.assets) == quote && e.shares == shares && e.requested_at == now, "{ctx}: unbond entry {e:?} vs quote {quote:?}");
                    check_eq!(e.claimable_at, now.saturating_add(before.unbond_period_ms), "{ctx}: claimable_at");
                    check_eq!(e.id, before.next_unbond_id, "{ctx}: unbond id not sequential");
                    prop_assert!(w.s.unbonds_of(&who).contains(&e), "{ctx}: entry not recorded");
                    check_eq!(w.s.reserve, before.reserve, "{ctx}: request_unbond moved the reserve");
                    check_eq!(w.s.unbonding_total, before.unbonding_total + e.assets, "{ctx}: unbonding_total after request");
                    check_eq!(w.s.distributable(now), d - e.assets, "{ctx}: distributable after request_unbond");
                    check_eq!(w.s.total_shares, before.total_shares - shares, "{ctx}: supply after request_unbond");
                    Done { label: "request_unbond", changed: true, rate: RateRule::Unbond { left: before.total_shares - shares }, actors: vec![who] }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::ZeroAmount => shares.is_zero(),
                        PoolError::InsufficientShares => shares > bal,
                        PoolError::BelowMinimum { min } => *min == u(MIN_PAYOUT) && quote.is_some_and(|q| q < u(MIN_PAYOUT)),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: request_unbond of {shares} (balance {bal}, quote {quote:?}) refused without cause: {e:?}");
                    Done { label: "request_unbond/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::Claim { pick, payout_fails } => {
            let holders: Vec<ActorId> = w.s.unbonds.keys().copied().collect();
            let who = if holders.is_empty() || *pick >= 224 { a(*pick as u64 % OWNERS + 1) } else { holders[*pick as usize % holders.len()] };
            let list = w.s.unbonds_of(&who);
            let target = if list.is_empty() { None } else { Some(list[*pick as usize % list.len()].clone()) };
            let id = target.as_ref().map(|e| e.id).unwrap_or(*pick as u64 + 1_000);
            let d = w.s.distributable(now);
            match w.s.take_unbond(who, id, now) {
                Ok(e) => {
                    check_eq!(Some(&e), target.as_ref(), "{ctx}: took a different entry");
                    prop_assert!(now >= e.claimable_at, "{ctx}: claimed before claimable_at");
                    prop_assert!(!w.s.unbonds_of(&who).iter().any(|x| x.id == id), "{ctx}: entry still listed after claim");
                    check_eq!(w.s.reserve, before.reserve - e.assets, "{ctx}: reserve after claim");
                    check_eq!(w.s.unbonding_total, before.unbonding_total - e.assets, "{ctx}: unbonding_total after claim");
                    check_eq!(w.s.distributable(now), d, "{ctx}: claim moved distributable");
                    if *payout_fails {
                        w.s.restore_unbond(who, e);
                        hit(w, "claim+restore");
                        Done { label: "claim+restore", changed: false, rate: RateRule::Same, actors: none }
                    } else {
                        hit(w, "claim");
                        w.value_out += e.assets;
                        Done { label: "claim", changed: true, rate: RateRule::Same, actors: none }
                    }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::UnbondNotFound => target.is_none(),
                        PoolError::UnbondNotReady { claimable_at } => target.as_ref().is_some_and(|t| t.claimable_at == *claimable_at && now < *claimable_at),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: claim of {id} refused without cause: {e:?} (entry {target:?})");
                    Done { label: "claim/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::CollectFees { payout_fails } => {
            // Mirrors `Pool::collect_fees`, which edits the state inline.
            let amount = w.s.fees_accrued;
            if amount.is_zero() {
                Done { label: "collect_fees/err", changed: false, rate: RateRule::Same, actors: none }
            } else {
                prop_assert!(amount <= w.s.reserve, "{ctx}: fees {amount} exceed the reserve {}", w.s.reserve);
                let d = w.s.distributable(now);
                w.s.fees_accrued = U256::zero();
                w.s.reserve -= amount;
                check_eq!(w.s.distributable(now), d, "{ctx}: collecting fees moved distributable");
                if *payout_fails {
                    w.s.fees_accrued = amount;
                    w.s.reserve = w.s.reserve.saturating_add(amount);
                    hit(w, "collect_fees+revert");
                    Done { label: "collect_fees+revert", changed: false, rate: RateRule::Same, actors: none }
                } else {
                    hit(w, "collect_fees");
                    w.value_out += amount;
                    Done { label: "collect_fees", changed: true, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::Advance { dt_ms } => {
            let later = (now + dt_ms).min(T0 + HORIZON_MS);
            let locked_before = w.s.locked_rewards(now);
            w.now = later;
            hit(w, "advance");
            prop_assert!(w.s.locked_rewards(later) <= locked_before, "{ctx}: locked rewards grew with time");
            Done { label: "advance", changed: true, rate: RateRule::Up, actors: none }
        }
        Op::Transfer { from, to, pct } => {
            let (from, to) = (a(*from), a(*to));
            let bal = w.s.balance_of(&from);
            let value = pct_of(bal, *pct);
            let to_before = w.s.balance_of(&to);
            match w.s.transfer_shares(from, to, value) {
                Ok(()) => {
                    hit(w, "transfer");
                    if from != to {
                        check_eq!(w.s.balance_of(&from), bal - value, "{ctx}: sender balance");
                        check_eq!(w.s.balance_of(&to), to_before + value, "{ctx}: receiver balance");
                    } else {
                        check_eq!(w.s.balance_of(&from), bal, "{ctx}: self transfer changed the balance");
                    }
                    check_eq!(w.s.total_shares, before.total_shares, "{ctx}: transfer changed supply");
                    Done { label: "transfer", changed: true, rate: RateRule::Same, actors: vec![from, to] }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::Paused => w.s.paused,
                        PoolError::ZeroAmount => value.is_zero(),
                        PoolError::InsufficientBalance => value > bal,
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: transfer of {value} (balance {bal}) refused without cause: {e:?}");
                    Done { label: "transfer/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::ProposeSession { owner, key, duration_secs, actions } => {
            let (owner, key) = (a(*owner), a(*key));
            let taken = bound_elsewhere(&w.s, key, owner, now);
            match w.s.propose_session(owner, key, *duration_secs, actions.clone(), now) {
                Ok(sess) => {
                    hit(w, "propose_session");
                    prop_assert!(key != owner && *duration_secs >= 1 && *duration_secs <= MAX_SESSION_SECS && !taken, "{ctx}: proposal accepted against the rules");
                    check_eq!(&sess, &Session { key, expires_at: now + duration_secs * 1000, actions: actions.clone() }, "{ctx}: proposal contents");
                    check_eq!(w.s.pending_sessions.get(&owner), Some(&sess), "{ctx}: proposal not stored");
                    Done { label: "propose_session", changed: true, rate: RateRule::Same, actors: none }
                }
                Err(e) => {
                    prop_assert!(e == PoolError::BadSession && (key == owner || *duration_secs == 0 || *duration_secs > MAX_SESSION_SECS || taken), "{ctx}: proposal refused without cause: {e:?}");
                    Done { label: "propose_session/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::AcceptSession { pick } => {
            let proposals: Vec<(ActorId, ActorId)> = w.s.pending_sessions.iter().map(|(o, p)| (*o, p.key)).collect();
            let (owner, key) = if proposals.is_empty() || *pick >= 224 {
                (a(*pick as u64 % OWNERS + 1), a((*pick as u64 / 4) % KEYS + 1))
            } else {
                proposals[*pick as usize % proposals.len()]
            };
            let proposal = w.s.pending_sessions.get(&owner).filter(|p| p.key == key).cloned();
            let taken = bound_elsewhere(&w.s, key, owner, now);
            // A dead binding of this key to somebody else is freed by the acceptance.
            let dead = w.s.session_keys.get(&key).copied().filter(|other| *other != owner && !taken);
            match w.s.accept_session(key, owner, now) {
                Ok(sess) => {
                    hit(w, "accept_session");
                    prop_assert!(proposal.as_ref() == Some(&sess) && now < sess.expires_at && !taken, "{ctx}: acceptance against the rules: {sess:?}");
                    check_eq!(w.s.sessions.get(&owner), Some(&sess), "{ctx}: session not active");
                    check_eq!(w.s.session_keys.get(&key), Some(&owner), "{ctx}: key not bound");
                    prop_assert!(!w.s.pending_sessions.contains_key(&owner), "{ctx}: proposal not consumed");
                    if let Some(other) = dead {
                        prop_assert!(!w.s.sessions.contains_key(&other), "{ctx}: dead session of {other:?} survived the key's re-binding");
                    }
                    Done { label: "accept_session", changed: true, rate: RateRule::Same, actors: none }
                }
                Err(e) => {
                    let justified = match &e {
                        PoolError::BadSession => proposal.is_none() || taken,
                        PoolError::SessionExpired => proposal.as_ref().is_some_and(|p| now >= p.expires_at),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: acceptance refused without cause: {e:?}");
                    Done { label: "accept_session/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::RevokeSession { owner } => {
            let owner = a(*owner);
            let had = w.s.sessions.contains_key(&owner) || w.s.pending_sessions.contains_key(&owner);
            let revoked = w.s.revoke_session(owner);
            check_eq!(revoked, had, "{ctx}: revoke_session return value");
            prop_assert!(!w.s.sessions.contains_key(&owner) && !w.s.pending_sessions.contains_key(&owner), "{ctx}: session survived revoke");
            if had { hit(w, "revoke_session"); }
            Done { label: "revoke_session", changed: had, rate: RateRule::Same, actors: none }
        }
        Op::SetConfig { fee_bps, unbond_secs, vesting_secs } => {
            let valid = *fee_bps <= MAX_FEE_BPS && *unbond_secs <= MAX_UNBOND_SECS && (MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(vesting_secs);
            match w.s.set_config(*fee_bps, *unbond_secs, *vesting_secs) {
                Ok(()) => {
                    hit(w, "set_config");
                    prop_assert!(valid, "{ctx}: invalid config accepted");
                    check_eq!((w.s.instant_fee_bps, w.s.unbond_period_ms, w.s.vesting_period_ms), (*fee_bps, unbond_secs * 1000, vesting_secs * 1000), "{ctx}: config not applied");
                    Done { label: "set_config", changed: true, rate: RateRule::Same, actors: none }
                }
                Err(e) => {
                    prop_assert!(e == PoolError::BadConfig && !valid, "{ctx}: config refused without cause: {e:?}");
                    Done { label: "set_config/err", changed: false, rate: RateRule::Same, actors: none }
                }
            }
        }
        Op::SetPaused(p) => {
            let changed = w.s.paused != *p;
            w.s.paused = *p;
            if changed { hit(w, "pause/resume"); }
            Done { label: "pause/resume", changed, rate: RateRule::Same, actors: none }
        }
    })
}

fn run(cfg: &Cfg, steps: &[Step]) -> Result<BTreeMap<&'static str, u32>, TestCaseError> {
    let s = PoolState::new(a(1), "Vale kVARA".into(), "kVARA".into(), 12, cfg.fee_bps, cfg.unbond_secs, cfg.vesting_secs, u(cfg.initial_rate));
    check_eq!(s.instant_fee_bps, cfg.fee_bps.min(MAX_FEE_BPS), "constructor did not clamp the fee");
    check_eq!(s.unbond_period_ms, cfg.unbond_secs.min(MAX_UNBOND_SECS) * 1000, "constructor did not clamp the unbond period");
    check_eq!(s.vesting_period_ms, cfg.vesting_secs.clamp(MIN_VESTING_SECS, MAX_VESTING_SECS) * 1000, "constructor did not clamp the vesting period");
    check_eq!(s.base_rate, u(cfg.initial_rate).max(SCALE), "constructor did not clamp the initial rate");
    let mut w = World { s, now: T0, value_in: U256::zero(), value_out: U256::zero(), hits: BTreeMap::new() };
    invariants(&w, "initial")?;

    for (i, step) in steps.iter().enumerate() {
        let ctx = format!("step {i} {:?} at t+{}ms", step.op, w.now - T0);
        let before = snap(&w.s);
        let rate_before = w.s.rate_at(w.now);
        let claims_before = claims(&w.s, w.now);

        let done = apply(&mut w, &step.op, &before, &ctx)?;
        let ctx = format!("{ctx} [{}]", done.label);
        let now = w.now;
        let rate_after = w.s.rate_at(now);

        // Errors and immediate reverts leave no trace (property 9 and atomicity). One known
        // exception: `burn_shares` refreshes `base_rate` when the last share goes and
        // `revert_unstake` does not put the old value back. The check allows exactly that
        // field to move, and only to the rate the pool had.
        if !done.changed {
            let after = snap(&w.s);
            let mut expect = before.clone();
            if after.base_rate != before.base_rate {
                prop_assert!(done.label == "unstake+revert" && after.base_rate == rate_before, "{ctx}: base_rate moved from {} to {} (rate was {rate_before})", before.base_rate, after.base_rate);
                expect.base_rate = after.base_rate;
            }
            prop_assert!(after == expect, "{ctx}: state changed although it should not have\n before: {before:#?}\n after: {after:#?}");
        }

        // Property 3 (and 8): the rate never falls, and only certain ops may move it.
        match done.rate {
            RateRule::Same => check_eq!(rate_after, rate_before, "{ctx}: rate moved"),
            RateRule::Up => prop_assert!(rate_after >= rate_before, "{ctx}: rate fell from {rate_before} to {rate_after}"),
            RateRule::Empty => {
                prop_assert!(rate_after >= rate_before, "{ctx}: emptying the pool lowered the rate from {rate_before} to {rate_after}");
                prop_assert!(rate_after - rate_before <= empty_tick(), "{ctx}: emptying the pool moved the rate by {} > {}", rate_after - rate_before, empty_tick());
            }
            RateRule::Revert { old_base } => {
                prop_assert!(rate_after >= rate_before, "{ctx}: revert lowered the rate from {rate_before} to {rate_after}");
                let nudge = (VIRTUAL_SHARES * (rate_before - old_base) + SCALE) / (w.s.total_shares + VIRTUAL_SHARES) + U256::from(2u64);
                prop_assert!(rate_after - rate_before <= nudge, "{ctx}: revert's base_rate refresh moved the rate by {} > {nudge}", rate_after - rate_before);
            }
            RateRule::Unbond { left } => {
                prop_assert!(rate_after >= rate_before, "{ctx}: unbond lowered the rate from {rate_before} to {rate_after}");
                let tick = if left.is_zero() { empty_tick() } else { SCALE / (left + VIRTUAL_SHARES) + U256::one() };
                prop_assert!(rate_after - rate_before <= tick, "{ctx}: unbond moved the rate by {} > one rounding tick {tick}", rate_after - rate_before);
            }
        }

        // No dilution: whoever did not take part keeps at least their claim.
        for (who, claim) in &claims_before {
            if done.actors.contains(who) { continue; }
            let after = w.s.assets_for(w.s.balance_of(who), now).expect("assets_for overflow");
            prop_assert!(after >= *claim, "{ctx}: bystander {who:?} diluted from {claim} to {after}");
        }

        invariants(&w, &ctx)?;
        if let Some(label) = probes(&w, step.probe, &ctx)? {
            hit(&mut w, label);
        }
    }
    Ok(w.hits)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, max_shrink_iters: 20_000, .. ProptestConfig::default() })]

    #[test]
    fn pool_state_machine(cfg in cfg(), steps in prop::collection::vec(step(), 1..=60)) {
        run(&cfg, &steps)?;
    }
}

/// Guards the sequence test against vacuity: over a fixed batch of random sequences every
/// state-changing operation, and every revert path, must have succeeded at least once.
#[test]
fn every_operation_is_exercised() {
    let total = RefCell::new(BTreeMap::<&'static str, u32>::new());
    let mut runner = TestRunner::new_with_rng(Config { cases: 200, max_shrink_iters: 20_000, ..Config::default() }, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    runner
        .run(&(cfg(), prop::collection::vec(step(), 1..=60)), |(cfg, steps)| {
            let hits = run(&cfg, &steps)?;
            let mut t = total.borrow_mut();
            for (k, v) in hits { *t.entry(k).or_default() += v; }
            Ok(())
        })
        .unwrap();
    let total = total.into_inner();
    eprintln!("coverage over 200 sequences: {total:#?}");
    let expected = [
        "stake", "unstake", "unstake+revert", "fund", "request_unbond", "claim", "claim+restore", "collect_fees", "collect_fees+revert",
        "advance", "transfer", "propose_session", "accept_session", "revoke_session", "set_config", "pause/resume",
    ];
    let missing: Vec<_> = expected.iter().filter(|k| total.get(*k).copied().unwrap_or(0) == 0).collect();
    assert!(missing.is_empty(), "operations never exercised: {missing:?}; coverage {total:?}");
}
