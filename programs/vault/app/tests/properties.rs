//! Property tests for the host-testable `VaultState` machine.
//!
//! A random sequence of operations (2-4 actors, token amounts of 1e3..1e16 base units, up to
//! 400 days of block time) is driven through `VaultState`, and after every step the whole
//! invariant set below is checked. The vault's flows are asynchronous on chain (a token call
//! sits between the pre-await and post-await halves), so the harness models that: a deposit is
//! checked now and settled (or refused) up to three operations later, and a redeem, claim or
//! fee collection completes, or fails and is compensated, up to three operations later, with
//! other operations interleaved. A redeem is `begin_redeem -> (finish_redeem | revert_redeem)`.
//!
//! Quotes use the virtual offset: `(distributable + VA) / (total_shares + VS)` with
//! `VS = VIRTUAL_SHARES` and `VA = ceil(VS * base_rate / SCALE)`, and an empty vault prices
//! nothing but the virtual pair. Consequences used below: `rate_at(now) >= base_rate`; burning
//! the last share re-quotes at `ceil`, so the rate may tick up by at most `SCALE / VS` there;
//! and holders' claims sum to at most `distributable` (the virtual shares' slice stays behind).
//!
//! Properties (numbered as in the request):
//!  1. Solvency: `holdings >= fees_accrued + fees_pending + unbonding_total + locked(now)`,
//!     `distributable(now) <= holdings`, and `holdings + in_flight` equals the tokens that
//!     physically entered the vault minus what left it (an external ledger kept by the harness,
//!     where `in_flight` is what was taken out of holdings for a transfer not yet completed).
//!  2. Conservation: `total_shares == sum(balances)`, no zero balance is stored,
//!     `unbonding_total == sum(unbond.assets)`, `fees_pending == sum(fees of redeems in
//!     flight)`, unbond and unconfirmed ids are unique, ascending and below their counters,
//!     and `sum(assets_for(balance)) <= distributable` (nothing over-promised).
//!  3. The rate never decreases: not over time, not across any operation. Exact statement per
//!     operation: `settle_deposit`, `begin_redeem`, `request_unbond` may raise it by rounding
//!     (in the vault's favour), and by at most `SCALE / VS` when they burn the last share; time
//!     may raise it (vesting); everything else leaves it untouched (`fund` moves nothing at
//!     the funding instant, `finish_redeem` only reclassifies a fee). `rate_at(now) >=
//!     base_rate`. The one exception is a *delayed* `revert_redeem`: it re-mints the shares at
//!     the price they were burned at, so rewards vested in the meantime are shared with the
//!     reverted holder; the fall is bounded by the rounding of that price, and afterwards the
//!     holders' claims may exceed `distributable` by at most `VS / shares + 1` units (a full
//!     exit can then be refused with `InsufficientReserve` for that residue).
//!  4. Round trip never profits: deposit A then redeem all the minted shares in the same
//!     instant returns <= A and leaves `distributable` and the rate at least where they were;
//!     on an empty vault the residual left to nobody is at most the virtual slice.
//!  5. `assets_for(shares_for(A)) <= A` and `shares_for(assets_for(s)) <= s`.
//!  6. `locked_rewards` is non-increasing in time, bounded by `vest_amount`, and zero at and
//!     after `vest_end_ms`.
//!  7. `fund` releases nothing at the funding instant (`locked` grows by exactly the amount),
//!     never moves `vest_end_ms` past `now + vesting_period_ms` unless the running tranche
//!     already ended later (possible after `set_config` shrank the period), and a 1-unit
//!     fund never moves the end of a running tranche whose locked amount is at least
//!     `period - time_left` (base units vs ms; below that the stretch is bounded by
//!     `(period - time_left) / (locked + 1)` ms, so the literal "never" does not hold).
//!  8. `request_unbond` is rate neutral: holdings unchanged, distributable down by exactly the
//!     assets locked, rate up by at most one rounding tick, other holders' claims untouched.
//!  9. `revert_redeem` / `restore_unbond` / `restore_fees` restore the exact prior state,
//!     compared field by field, whenever nothing else changed the state in between. Known
//!     exception, allowed explicitly: a redeem that burned the last share leaves `base_rate`
//!     refreshed to the pre-redeem rate (visible only as an upward nudge on the rate). With
//!     interleaved changes the compensation is checked piecewise.
//! 10. Sessions: `sessions` and `session_keys` are mutual inverses, a key serves one owner,
//!     and `actor_for` routes a key to its owner only while the session is live and allows the
//!     action, and routes everybody else (expired keys included) to themselves.
//! Plus: every `Err` leaves the state untouched, every `Err` is justified by its documented
//! precondition, and no operation dilutes a holder who did not take part in it (except the
//! delayed revert above).

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use sails_rs::collections::{BTreeMap, BTreeSet};
use sails_rs::prelude::*;
use std::cell::RefCell;
use vault_app::*;

const T0: u64 = 1_700_000_000_000;
const DAY_MS: u64 = 86_400_000;
const HORIZON_MS: u64 = 400 * DAY_MS;
/// Actors 1..=OWNERS hold positions; session keys are drawn from 1..=KEYS so a key can be
/// another owner (allowed) or the owner itself (rejected).
const OWNERS: u64 = 4;
const KEYS: u64 = 8;
const UNDERLYING: u64 = 99;
const MIN: u128 = MIN_DEPOSIT as u128;

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
/// The most the rate may tick up when the last share is burned and the vault re-quotes at
/// `ceil(VS * rate / SCALE)`.
fn empty_tick() -> U256 { SCALE / VIRTUAL_SHARES + U256::one() }

// ---------------------------------------------------------------------------
// Snapshot: every field of `VaultState`, so a revert can be checked exactly and a state cloned.
// A struct literal on purpose: a field added to `VaultState` fails this build instead of
// silently escaping the comparison.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snap {
    admin: ActorId,
    underlying: ActorId,
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
    holdings: U256,
    fees_accrued: U256,
    fees_pending: U256,
    unbonding_total: U256,
    unconfirmed: Vec<Unconfirmed>,
    next_unconfirmed_id: u64,
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

fn snap(s: &VaultState) -> Snap {
    Snap {
        admin: s.admin,
        underlying: s.underlying,
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
        holdings: s.holdings,
        fees_accrued: s.fees_accrued,
        fees_pending: s.fees_pending,
        unbonding_total: s.unbonding_total,
        unconfirmed: s.unconfirmed.clone(),
        next_unconfirmed_id: s.next_unconfirmed_id,
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

fn restore(sn: &Snap) -> VaultState {
    VaultState {
        admin: sn.admin,
        underlying: sn.underlying,
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
        holdings: sn.holdings,
        fees_accrued: sn.fees_accrued,
        fees_pending: sn.fees_pending,
        unbonding_total: sn.unbonding_total,
        unconfirmed: sn.unconfirmed.clone(),
        next_unconfirmed_id: sn.next_unconfirmed_id,
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

/// A deposit: 1e3 .. 1e16 base units, with the minimum and just below it pinned.
fn deposit_amount() -> impl Strategy<Value = u128> {
    prop_oneof![8 => log_uniform(3, 15), 1 => Just(MIN), 1 => Just(MIN - 1)]
}

/// A reward: realistic tranches plus small ones and 1-unit dust (for the vesting properties).
fn reward_amount() -> impl Strategy<Value = u128> {
    prop_oneof![7 => log_uniform(3, 15), 2 => log_uniform(0, 2), 1 => Just(1)]
}

/// Time between operations: same block, a block, within a day, weeks.
fn dt() -> impl Strategy<Value = u64> {
    prop_oneof![2 => Just(0u64), 3 => 1_000u64..=3_000, 3 => 3_000u64..=DAY_MS, 3 => DAY_MS..=40 * DAY_MS]
}

/// Share of a balance: partial, all, and occasionally none or more than held.
fn pct() -> impl Strategy<Value = u8> {
    prop_oneof![6 => 1u8..=99, 3 => Just(100u8), 1 => 0u8..=120]
}

/// Operations that pass before an async flow completes: usually none (same block), sometimes
/// a few, so completions interleave with other traffic.
fn after() -> impl Strategy<Value = u8> {
    prop_oneof![5 => Just(0u8), 3 => 1u8..=3]
}

fn owner() -> impl Strategy<Value = u64> { 1..=OWNERS }
fn key() -> impl Strategy<Value = u64> { 1..=KEYS }

fn actions() -> impl Strategy<Value = Vec<SessionAction>> {
    prop::sample::subsequence(vec![SessionAction::Deposit, SessionAction::Redeem, SessionAction::Unbond, SessionAction::Claim], 1..=4)
}

fn duration() -> impl Strategy<Value = u64> {
    prop_oneof![4 => 1u64..=MAX_SESSION_SECS, 1 => Just(0u64), 1 => Just(MAX_SESSION_SECS + 1)]
}

fn kind() -> impl Strategy<Value = UnconfirmedKind> {
    prop_oneof![Just(UnconfirmedKind::Deposit), Just(UnconfirmedKind::Rewards), Just(UnconfirmedKind::Payout)]
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
    /// `check_deposit` now; the tokens arrive and `settle_deposit` runs `after` operations later.
    Deposit { who: u64, amount: u128, after: u8 },
    /// `begin_redeem` now; the payout lands (`finish_redeem`) or fails (`revert_redeem`)
    /// `after` operations later.
    Redeem { who: u64, pct: u8, payout_fails: bool, after: u8 },
    Fund { amount: u128 },
    RequestUnbond { who: u64, pct: u8 },
    /// `take_unbond` now; the payout completes (or fails and is restored) `after` operations
    /// later. `pick` selects one of the actors with unbonds and one of their entries; a slice
    /// of the range picks a random actor instead, to reach the not-found paths.
    Claim { pick: u8, payout_fails: bool, after: u8 },
    /// `take_fees` now; the payout completes (or fails and is restored) `after` operations later.
    CollectFees { payout_fails: bool, after: u8 },
    Advance { dt_ms: u64 },
    Transfer { from: u64, to: u64, pct: u8 },
    ProposeSession { owner: u64, key: u64, duration_secs: u64, actions: Vec<SessionAction> },
    /// `pick` selects one of the pending proposals; a slice of the range picks a random
    /// (key, owner) pair instead, to reach the rejection paths.
    AcceptSession { pick: u8 },
    RevokeSession { owner: u64 },
    SetConfig { fee_bps: u32, unbond_secs: u64, vesting_secs: u64 },
    SetPaused(bool),
    RecordUnconfirmed { who: u64, kind: UnconfirmedKind, amount: u128 },
    /// `pick` selects one of the recorded entries; a slice of the range picks an unknown id.
    TakeUnconfirmed { pick: u8 },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        5 => (owner(), deposit_amount(), after()).prop_map(|(who, amount, after)| Op::Deposit { who, amount, after }),
        4 => (owner(), pct(), any::<bool>(), after()).prop_map(|(who, pct, payout_fails, after)| Op::Redeem { who, pct, payout_fails, after }),
        3 => reward_amount().prop_map(|amount| Op::Fund { amount }),
        3 => (owner(), pct()).prop_map(|(who, pct)| Op::RequestUnbond { who, pct }),
        3 => (any::<u8>(), any::<bool>(), after()).prop_map(|(pick, payout_fails, after)| Op::Claim { pick, payout_fails, after }),
        1 => (any::<bool>(), after()).prop_map(|(payout_fails, after)| Op::CollectFees { payout_fails, after }),
        6 => dt().prop_map(|dt_ms| Op::Advance { dt_ms }),
        2 => (owner(), owner(), pct()).prop_map(|(from, to, pct)| Op::Transfer { from, to, pct }),
        2 => (owner(), key(), duration(), actions()).prop_map(|(owner, key, duration_secs, actions)| Op::ProposeSession { owner, key, duration_secs, actions }),
        2 => any::<u8>().prop_map(|pick| Op::AcceptSession { pick }),
        1 => owner().prop_map(|owner| Op::RevokeSession { owner }),
        1 => (0u32..=MAX_FEE_BPS + 10, 0u64..=MAX_UNBOND_SECS + 1, MIN_VESTING_SECS - 1..=MAX_VESTING_SECS + 1)
            .prop_map(|(fee_bps, unbond_secs, vesting_secs)| Op::SetConfig { fee_bps, unbond_secs, vesting_secs }),
        1 => any::<bool>().prop_map(Op::SetPaused),
        1 => (owner(), kind(), deposit_amount()).prop_map(|(who, kind, amount)| Op::RecordUnconfirmed { who, kind, amount }),
        1 => any::<u8>().prop_map(|pick| Op::TakeUnconfirmed { pick }),
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
    (op(), deposit_amount()).prop_map(|(op, probe)| Step { op, probe })
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// How the rate is allowed to move across one step.
#[derive(Debug, Clone, Copy)]
enum RateRule {
    Same,
    Up,
    /// The last share was burned: the vault re-quotes at `ceil(VS * rate / SCALE)`, up by at
    /// most `SCALE / VS`.
    Empty,
    /// `request_unbond` with `left` shares remaining: up by at most one rounding tick.
    Unbond { left: U256 },
    /// An exact `revert_redeem`: the pre-redeem state's rate now, plus, when the redeem had
    /// burned the last share, the nudge from the refreshed `base_rate`.
    Restore { rate_restored: U256, nudge: U256 },
    /// A delayed `revert_redeem`: re-minting `shares` at the price they were burned at may
    /// lower the rate, by at most the rounding of that price (`SCALE / shares`).
    Revert { shares: U256, rate_at_begin: U256 },
}

struct Done {
    label: &'static str,
    /// The state was meant to change. When false the step must have left it byte-identical.
    changed: bool,
    rate: RateRule,
    /// Holders whose claim legitimately moved; everybody else must not be diluted.
    actors: Vec<ActorId>,
    /// A revert re-prices everybody: skip the bystander check for this step.
    skip_dilution: bool,
}

impl Done {
    fn new(label: &'static str, changed: bool, rate: RateRule, actors: Vec<ActorId>) -> Self {
        Done { label, changed, rate, actors, skip_dilution: false }
    }
}

/// The second half of an async flow, waiting for its token call to complete.
#[derive(Debug, Clone)]
enum Pending {
    Settle { who: ActorId, assets: U256 },
    Redeem { who: ActorId, shares: U256, p: RedeemPreview, rate_at_begin: U256, fails: bool },
    Claim { who: ActorId, entry: Unbond, fails: bool },
    Fees { amount: U256, fails: bool },
}

struct Queued {
    after: u8,
    /// State before the operation that started the flow, and the mutation count right after
    /// it: when the count is unchanged at completion time a compensation must be exact.
    before: Snap,
    mutations: u32,
    pending: Pending,
}

struct World {
    s: VaultState,
    now: u64,
    /// Tokens that entered the vault (deposits, rewards) and left it (completed payouts).
    value_in: U256,
    value_out: U256,
    /// Taken out of `holdings` for a transfer that has not completed yet.
    in_flight: U256,
    /// Number of state-changing steps so far (time advances excluded).
    mutations: u32,
    /// A delayed revert re-priced the vault: `rate >= base_rate` no longer has to hold.
    repriced: bool,
    /// Units by which holders' claims may exceed `distributable` after delayed reverts: each
    /// re-mints at the pre-await price, up to `VS / shares + 1` below the virtual pair's
    /// price. Cleared when the vault empties (the next deposit sweeps the difference).
    promise_slack: U256,
    queue: Vec<Queued>,
    hits: BTreeMap<&'static str, u32>,
}

type Check = Result<(), TestCaseError>;

fn claims(s: &VaultState, now: u64) -> BTreeMap<ActorId, U256> {
    s.balances.iter().map(|(who, bal)| (*who, s.assets_for(*bal, now).unwrap_or(U256::MAX))).collect()
}

/// True when `key` is held by a live session of an owner other than `owner`.
fn bound_elsewhere(s: &VaultState, key: ActorId, owner: ActorId, now: u64) -> bool {
    s.session_keys.get(&key).is_some_and(|other| *other != owner && s.sessions.get(other).is_some_and(|x| now < x.expires_at))
}

fn session_invariants(s: &VaultState, now: u64, ctx: &str) -> Check {
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
    let all = [SessionAction::Deposit, SessionAction::Redeem, SessionAction::Unbond, SessionAction::Claim];
    for id in 1..=KEYS {
        let sender = a(id);
        let live = s.session_keys.get(&sender).map(|owner| (*owner, &s.sessions[owner])).filter(|(_, x)| now < x.expires_at);
        for act in all {
            let expect = match live {
                None => Ok(sender),
                Some((owner, x)) if x.actions.contains(&act) => Ok(owner),
                Some(_) => Err(VaultError::SessionNotAllowed),
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
    let owed = [s.fees_accrued, s.fees_pending, s.unbonding_total, locked].iter().try_fold(U256::zero(), |acc, x| acc.checked_add(*x)).expect("owed overflow");
    prop_assert!(s.holdings >= owed, "{ctx}: insolvent: holdings {} < fees {} + pending fees {} + unbonding {} + locked {}", s.holdings, s.fees_accrued, s.fees_pending, s.unbonding_total, locked);
    prop_assert!(s.distributable(now) <= s.holdings, "{ctx}: distributable exceeds holdings");
    check_eq!(s.holdings + w.in_flight, w.value_in - w.value_out, "{ctx}: holdings + in-flight do not match the tokens that entered minus what left");

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
            prop_assert!(e.claimable_at >= e.requested_at && !e.assets.is_zero() && !e.shares.is_zero(), "{ctx}: malformed unbond {e:?}");
            unbonding += e.assets;
        }
    }
    check_eq!(s.unbonding_total, unbonding, "{ctx}: unbonding_total != sum of entries");
    let pending_fees = w.queue.iter().filter_map(|q| match &q.pending { Pending::Redeem { p, .. } => Some(p.fee), _ => None }).fold(U256::zero(), |acc, f| acc + f);
    check_eq!(s.fees_pending, pending_fees, "{ctx}: fees_pending != fees of the redeems in flight");
    let mut uids = BTreeSet::new();
    for pair in s.unconfirmed.windows(2) {
        prop_assert!(pair[0].id < pair[1].id, "{ctx}: unconfirmed entries not ascending");
    }
    for e in &s.unconfirmed {
        prop_assert!(e.id < s.next_unconfirmed_id && uids.insert(e.id), "{ctx}: unconfirmed id {} duplicated or beyond its counter", e.id);
    }

    let d = s.distributable(now);
    if !s.total_shares.is_zero() {
        let promised = claims(s, now).values().fold(U256::zero(), |acc, c| acc.saturating_add(*c));
        prop_assert!(promised <= d + w.promise_slack, "{ctx}: holders are promised {promised} but only {d} is distributable (slack {})", w.promise_slack);
    }
    if !w.repriced {
        prop_assert!(s.rate_at(now) >= s.base_rate, "{ctx}: rate {} fell below the base rate {}", s.rate_at(now), s.base_rate);
    }
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

/// Properties 4 and 5 on a copy of the state, with the step's probe amount.
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

    if s.paused || probe < MIN {
        return Ok(None);
    }
    let who = a(OWNERS + 1); // a visitor with no position
    let d_before = s.distributable(now);
    let rate_before = s.rate_at(now);
    let shares_before = s.total_shares;
    let mut c = restore(&snap(s));
    let checked = c.check_deposit(amount, now);
    if sh.is_zero() {
        // The rate has outgrown the minimum deposit (rewards dwarf the principal): the deposit
        // would mint nothing and must be refused rather than swallowed.
        check_eq!(checked, Err(VaultError::BelowMinimum { min: u(MIN) }), "{ctx}: a zero-share deposit of {amount} was not refused");
        return Ok(Some("probe: zero-share deposit refused"));
    }
    check_eq!(checked, Ok(sh), "{ctx}: check_deposit quote");
    let minted = c.settle_deposit(who, amount, now);
    check_eq!(minted, Ok(sh), "{ctx}: settle_deposit minted other than quoted");
    let out = c.begin_redeem(who, sh, now);
    prop_assert!(out.is_ok(), "{ctx}: fresh deposit of {amount} ({sh} shares) cannot be redeemed: {out:?}");
    let out = out.unwrap();
    c.finish_redeem(&out);
    prop_assert!(out.assets <= amount, "{ctx}: round trip of {amount} returned {} gross", out.assets);
    prop_assert!(out.net + out.fee == out.assets && out.fee <= out.assets / U256::from(10u64), "{ctx}: fee split wrong: {out:?}");
    if shares_before.is_zero() {
        // An empty vault: whatever had vested with nobody holding is orphaned and the first
        // deposit sweeps it into fees; the round trip may leave behind at most the virtual
        // shares' slice of the deposit, which the next deposit sweeps in turn.
        let residual = c.distributable(now);
        let slice = amount * VIRTUAL_SHARES / (VIRTUAL_SHARES + sh) + U256::one();
        prop_assert!(residual <= slice, "{ctx}: round trip on an empty vault left {residual} distributable, more than the virtual slice {slice}");
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

fn enqueue(w: &mut World, after: u8, before: &Snap, pending: Pending) {
    w.queue.push(Queued { after, before: before.clone(), mutations: w.mutations + 1, pending });
}

/// Apply one operation, checking its own contract (post-conditions on success, a justified
/// error on failure). Generic properties are checked afterwards by `execute`.
fn apply(w: &mut World, op: &Op, before: &Snap, ctx: &str) -> Result<Done, TestCaseError> {
    let now = w.now;
    let none = Vec::new();
    Ok(match op {
        Op::Deposit { who, amount, after } => {
            let who = a(*who);
            let amount = u(*amount);
            let quote = w.s.shares_for(amount, now).expect("shares_for overflow");
            match w.s.check_deposit(amount, now) {
                Ok(shares) => {
                    hit(w, "check_deposit");
                    check_eq!(shares, quote, "{ctx}: check_deposit quote");
                    prop_assert!(!shares.is_zero(), "{ctx}: a deposit quoted zero shares was accepted");
                    // The tokens are pulled during the await; they arrive with the settlement.
                    enqueue(w, *after, before, Pending::Settle { who, assets: amount });
                    Done::new("check_deposit", false, RateRule::Same, none)
                }
                Err(e) => {
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::ZeroAmount => amount.is_zero(),
                        VaultError::BelowMinimum { min } => *min == u(MIN) && (amount < u(MIN) || quote.is_zero()),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: deposit of {amount} refused without cause: {e:?}");
                    Done::new("check_deposit/err", false, RateRule::Same, none)
                }
            }
        }
        Op::Redeem { who, pct, payout_fails, after } => {
            let who = a(*who);
            let bal = w.s.balance_of(&who);
            let shares = pct_of(bal, *pct);
            let d = w.s.distributable(now);
            let rate = w.s.rate_at(now);
            let quote = if shares <= w.s.total_shares { w.s.preview_redeem(shares, now).ok() } else { None };
            match w.s.begin_redeem(who, shares, now) {
                Ok(p) => {
                    hit(w, "begin_redeem");
                    check_eq!(Some(&p), quote.as_ref(), "{ctx}: redeem pays other than the preview");
                    prop_assert!(p.assets <= d, "{ctx}: redeem pays {} out of {d} distributable", p.assets);
                    prop_assert!(p.net + p.fee == p.assets && !p.net.is_zero(), "{ctx}: bad split {p:?}");
                    check_eq!(p.fee, p.assets * U256::from(w.s.instant_fee_bps) / U256::from(BPS), "{ctx}: fee is not instant_fee_bps of assets");
                    check_eq!(w.s.balance_of(&who), bal - shares, "{ctx}: balance after begin_redeem");
                    check_eq!(w.s.total_shares, before.total_shares - shares, "{ctx}: supply after begin_redeem");
                    check_eq!(w.s.holdings, before.holdings - p.net, "{ctx}: holdings after begin_redeem");
                    check_eq!(w.s.fees_pending, before.fees_pending + p.fee, "{ctx}: pending fees after begin_redeem");
                    check_eq!(w.s.fees_accrued, before.fees_accrued, "{ctx}: begin_redeem touched accrued fees");
                    check_eq!(w.s.distributable(now), d - p.assets, "{ctx}: distributable after begin_redeem");
                    w.in_flight += p.net;
                    let emptied = w.s.total_shares.is_zero();
                    enqueue(w, *after, before, Pending::Redeem { who, shares, p, rate_at_begin: rate, fails: *payout_fails });
                    Done::new("begin_redeem", true, if emptied { RateRule::Empty } else { RateRule::Up }, vec![who])
                }
                Err(e) => {
                    // Only after a delayed revert may a full exit find the vault a unit short.
                    let short = quote.as_ref().is_some_and(|q| q.assets > d) && w.repriced;
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::ZeroAmount => shares.is_zero() || quote.as_ref().is_some_and(|q| q.net.is_zero()),
                        VaultError::InsufficientShares => shares > bal,
                        VaultError::InsufficientReserve { available } => *available == d && short,
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: redeem of {shares} (balance {bal}, preview {quote:?}, distributable {d}) refused without cause: {e:?}");
                    if short { hit(w, "begin_redeem/short after delayed revert"); }
                    Done::new("begin_redeem/err", false, RateRule::Same, none)
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
                    check_eq!(w.s.holdings, before.holdings + amount, "{ctx}: holdings after fund");
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
                        prop_assert!(w.s.vest_end_ms <= end_before, "{ctx}: 1-unit fund stretched a running tranche from {end_before} to {} (locked {locked_before}, period {period}ms)", w.s.vest_end_ms);
                    }
                    Done::new("fund", true, RateRule::Same, none)
                }
                Err(e) => {
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::ZeroAmount => amount.is_zero(),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: fund of {amount} refused without cause: {e:?}");
                    Done::new("fund/err", false, RateRule::Same, none)
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
                    check_eq!(w.s.holdings, before.holdings, "{ctx}: request_unbond moved holdings");
                    check_eq!(w.s.unbonding_total, before.unbonding_total + e.assets, "{ctx}: unbonding_total after request");
                    check_eq!(w.s.distributable(now), d - e.assets, "{ctx}: distributable after request_unbond");
                    check_eq!(w.s.total_shares, before.total_shares - shares, "{ctx}: supply after request_unbond");
                    Done::new("request_unbond", true, RateRule::Unbond { left: before.total_shares - shares }, vec![who])
                }
                Err(e) => {
                    let short = quote.is_some_and(|q| q > d) && w.repriced;
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::ZeroAmount => shares.is_zero() || quote == Some(U256::zero()),
                        VaultError::InsufficientShares => shares > bal,
                        VaultError::InsufficientReserve { available } => *available == d && short,
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: request_unbond of {shares} (balance {bal}, quote {quote:?}, distributable {d}) refused without cause: {e:?}");
                    if short { hit(w, "request_unbond/short after delayed revert"); }
                    Done::new("request_unbond/err", false, RateRule::Same, none)
                }
            }
        }
        Op::Claim { pick, payout_fails, after } => {
            let holders: Vec<ActorId> = w.s.unbonds.keys().copied().collect();
            let who = if holders.is_empty() || *pick >= 224 { a(*pick as u64 % OWNERS + 1) } else { holders[*pick as usize % holders.len()] };
            let list = w.s.unbonds_of(&who);
            let target = if list.is_empty() { None } else { Some(list[*pick as usize % list.len()].clone()) };
            let id = target.as_ref().map(|e| e.id).unwrap_or(*pick as u64 + 1_000);
            let d = w.s.distributable(now);
            match w.s.take_unbond(who, id, now) {
                Ok(e) => {
                    hit(w, "take_unbond");
                    check_eq!(Some(&e), target.as_ref(), "{ctx}: took a different entry");
                    prop_assert!(now >= e.claimable_at, "{ctx}: claimed before claimable_at");
                    prop_assert!(!w.s.unbonds_of(&who).iter().any(|x| x.id == id), "{ctx}: entry still listed after claim");
                    check_eq!(w.s.holdings, before.holdings - e.assets, "{ctx}: holdings after take_unbond");
                    check_eq!(w.s.unbonding_total, before.unbonding_total - e.assets, "{ctx}: unbonding_total after take_unbond");
                    check_eq!(w.s.distributable(now), d, "{ctx}: take_unbond moved distributable");
                    w.in_flight += e.assets;
                    enqueue(w, *after, before, Pending::Claim { who, entry: e, fails: *payout_fails });
                    Done::new("take_unbond", true, RateRule::Same, none)
                }
                Err(e) => {
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::UnbondNotFound => target.is_none(),
                        VaultError::UnbondNotReady { claimable_at } => target.as_ref().is_some_and(|t| t.claimable_at == *claimable_at && now < *claimable_at),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: claim of {id} refused without cause: {e:?} (entry {target:?})");
                    Done::new("take_unbond/err", false, RateRule::Same, none)
                }
            }
        }
        Op::CollectFees { payout_fails, after } => {
            let fees = w.s.fees_accrued;
            let d = w.s.distributable(now);
            match w.s.take_fees() {
                Ok(amount) => {
                    hit(w, "take_fees");
                    check_eq!(amount, fees, "{ctx}: take_fees amount");
                    prop_assert!(w.s.fees_accrued.is_zero() && w.s.holdings == before.holdings - amount, "{ctx}: take_fees bookkeeping");
                    check_eq!(w.s.fees_pending, before.fees_pending, "{ctx}: take_fees touched the pending fees");
                    check_eq!(w.s.distributable(now), d, "{ctx}: take_fees moved distributable");
                    w.in_flight += amount;
                    enqueue(w, *after, before, Pending::Fees { amount, fails: *payout_fails });
                    Done::new("take_fees", true, RateRule::Same, none)
                }
                Err(e) => {
                    prop_assert!(e == VaultError::ZeroAmount && fees.is_zero(), "{ctx}: take_fees refused without cause: {e:?}");
                    Done::new("take_fees/err", false, RateRule::Same, none)
                }
            }
        }
        Op::Advance { dt_ms } => {
            let later = (now + dt_ms).min(T0 + HORIZON_MS);
            let locked_before = w.s.locked_rewards(now);
            w.now = later;
            hit(w, "advance");
            prop_assert!(w.s.locked_rewards(later) <= locked_before, "{ctx}: locked rewards grew with time");
            Done::new("advance", true, RateRule::Up, none)
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
                    Done::new("transfer", true, RateRule::Same, vec![from, to])
                }
                Err(e) => {
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::ZeroAmount => value.is_zero(),
                        VaultError::InsufficientBalance => value > bal,
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: transfer of {value} (balance {bal}) refused without cause: {e:?}");
                    Done::new("transfer/err", false, RateRule::Same, none)
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
                    Done::new("propose_session", true, RateRule::Same, none)
                }
                Err(e) => {
                    prop_assert!(e == VaultError::BadSession && (key == owner || *duration_secs == 0 || *duration_secs > MAX_SESSION_SECS || taken), "{ctx}: proposal refused without cause: {e:?}");
                    Done::new("propose_session/err", false, RateRule::Same, none)
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
                    Done::new("accept_session", true, RateRule::Same, none)
                }
                Err(e) => {
                    let justified = match &e {
                        VaultError::BadSession => proposal.is_none() || taken,
                        VaultError::SessionExpired => proposal.as_ref().is_some_and(|p| now >= p.expires_at),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: acceptance refused without cause: {e:?}");
                    Done::new("accept_session/err", false, RateRule::Same, none)
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
            Done::new("revoke_session", had, RateRule::Same, none)
        }
        Op::SetConfig { fee_bps, unbond_secs, vesting_secs } => {
            let valid = *fee_bps <= MAX_FEE_BPS && *unbond_secs <= MAX_UNBOND_SECS && (MIN_VESTING_SECS..=MAX_VESTING_SECS).contains(vesting_secs);
            match w.s.set_config(*fee_bps, *unbond_secs, *vesting_secs) {
                Ok(()) => {
                    hit(w, "set_config");
                    prop_assert!(valid, "{ctx}: invalid config accepted");
                    check_eq!((w.s.instant_fee_bps, w.s.unbond_period_ms, w.s.vesting_period_ms), (*fee_bps, unbond_secs * 1000, vesting_secs * 1000), "{ctx}: config not applied");
                    Done::new("set_config", true, RateRule::Same, none)
                }
                Err(e) => {
                    prop_assert!(e == VaultError::BadConfig && !valid, "{ctx}: config refused without cause: {e:?}");
                    Done::new("set_config/err", false, RateRule::Same, none)
                }
            }
        }
        Op::SetPaused(p) => {
            let changed = w.s.paused != *p;
            w.s.paused = *p;
            if changed { hit(w, "pause/resume"); }
            Done::new("pause/resume", changed, RateRule::Same, none)
        }
        Op::RecordUnconfirmed { who, kind, amount } => {
            let (who, amount) = (a(*who), u(*amount));
            let id = w.s.record_unconfirmed(who, *kind, amount);
            hit(w, "record_unconfirmed");
            check_eq!(id, before.next_unconfirmed_id, "{ctx}: unconfirmed id not sequential");
            check_eq!(w.s.unconfirmed.last(), Some(&Unconfirmed { id, owner: who, kind: *kind, assets: amount }), "{ctx}: entry not recorded");
            prop_assert!(w.s.holdings == before.holdings && w.s.total_shares == before.total_shares, "{ctx}: record_unconfirmed moved amounts");
            Done::new("record_unconfirmed", true, RateRule::Same, none)
        }
        Op::TakeUnconfirmed { pick } => {
            let target = if w.s.unconfirmed.is_empty() || *pick >= 224 { None } else { Some(w.s.unconfirmed[*pick as usize % w.s.unconfirmed.len()].clone()) };
            let id = target.as_ref().map(|e| e.id).unwrap_or(*pick as u64 + 1_000);
            match w.s.take_unconfirmed(id) {
                Ok(e) => {
                    hit(w, "take_unconfirmed");
                    check_eq!(Some(&e), target.as_ref(), "{ctx}: took a different entry");
                    prop_assert!(!w.s.unconfirmed.iter().any(|x| x.id == id), "{ctx}: entry still listed");
                    prop_assert!(w.s.holdings == before.holdings && w.s.total_shares == before.total_shares, "{ctx}: take_unconfirmed moved amounts");
                    Done::new("take_unconfirmed", true, RateRule::Same, none)
                }
                Err(e) => {
                    prop_assert!(e == VaultError::UnbondNotFound && target.is_none(), "{ctx}: take_unconfirmed({id}) refused without cause: {e:?}");
                    Done::new("take_unconfirmed/err", false, RateRule::Same, none)
                }
            }
        }
    })
}

/// Complete an async flow: the token call succeeded (tokens moved, fee reclassified) or failed
/// (state compensated, tokens stayed), or a settlement ran (or was refused) with the tokens.
fn fire(w: &mut World, q: Queued, ctx: &str) -> Result<Done, TestCaseError> {
    let now = w.now;
    let none = Vec::new();
    let exact = w.mutations == q.mutations;
    Ok(match q.pending {
        Pending::Settle { who, assets } => {
            let quote = w.s.shares_for(assets, now).expect("shares_for overflow");
            let (bal, supply, holdings) = (w.s.balance_of(&who), w.s.total_shares, w.s.holdings);
            match w.s.settle_deposit(who, assets, now) {
                Ok(shares) => {
                    hit(w, "settle_deposit");
                    w.value_in += assets;
                    check_eq!(shares, quote, "{ctx}: settled at other than the settlement-time price");
                    prop_assert!(!shares.is_zero(), "{ctx}: settled for zero shares");
                    check_eq!(w.s.balance_of(&who), bal + shares, "{ctx}: balance after settle");
                    check_eq!(w.s.total_shares, supply + shares, "{ctx}: supply after settle");
                    check_eq!(w.s.holdings, holdings + assets, "{ctx}: holdings after settle");
                    Done::new("settle_deposit", true, RateRule::Up, vec![who])
                }
                Err(e) => {
                    // The tokens had arrived; the handler hands them back, so the ledger nets out.
                    let justified = match &e {
                        VaultError::Paused => w.s.paused,
                        VaultError::BelowMinimum { min } => *min == u(MIN) && quote.is_zero(),
                        _ => false,
                    };
                    prop_assert!(justified, "{ctx}: settle_deposit refused without cause: {e:?}");
                    hit(w, "settle_deposit/refused");
                    Done::new("settle_deposit/refused", false, RateRule::Same, none)
                }
            }
        }
        Pending::Redeem { who, shares, p, rate_at_begin, fails } => {
            w.in_flight -= p.net;
            let (bal, supply, holdings, pending, accrued, d) = (w.s.balance_of(&who), w.s.total_shares, w.s.holdings, w.s.fees_pending, w.s.fees_accrued, w.s.distributable(now));
            if !fails {
                hit(w, "redeem paid");
                w.value_out += p.net;
                w.s.finish_redeem(&p);
                check_eq!(w.s.fees_pending, pending - p.fee, "{ctx}: pending fees after finish_redeem");
                check_eq!(w.s.fees_accrued, accrued + p.fee, "{ctx}: accrued fees after finish_redeem");
                check_eq!(w.s.distributable(now), d, "{ctx}: finish_redeem moved distributable");
                Done::new("redeem paid", true, RateRule::Same, none)
            } else {
                w.s.revert_redeem(who, shares, &p);
                check_eq!(w.s.balance_of(&who), bal + shares, "{ctx}: shares not given back");
                check_eq!(w.s.total_shares, supply + shares, "{ctx}: supply after revert");
                check_eq!(w.s.holdings, holdings + p.net, "{ctx}: holdings after revert");
                check_eq!(w.s.fees_pending, pending - p.fee, "{ctx}: pending fees after revert");
                check_eq!(w.s.fees_accrued, accrued, "{ctx}: revert touched accrued fees");
                check_eq!(w.s.distributable(now), d + p.assets, "{ctx}: revert did not give the whole payout back to distributable");
                if exact {
                    hit(w, "revert_redeem exact");
                    let mut expect = q.before.clone();
                    let after = snap(&w.s);
                    let mut nudge = U256::zero();
                    if after.base_rate != expect.base_rate {
                        prop_assert!(after.base_rate == rate_at_begin, "{ctx}: base_rate moved from {} to {} (rate at begin {rate_at_begin})", expect.base_rate, after.base_rate);
                        nudge = (VIRTUAL_SHARES * (rate_at_begin - expect.base_rate) + SCALE) / (after.total_shares + VIRTUAL_SHARES) + U256::from(2u64);
                        expect.base_rate = after.base_rate;
                    }
                    prop_assert!(after == expect, "{ctx}: revert_redeem did not restore the state\n before: {expect:#?}\n after: {after:#?}");
                    let rate_restored = restore(&q.before).rate_at(now);
                    // Everything is back where it was before the redeem, rounding included, so
                    // the bystander check (relative to the post-begin state) does not apply.
                    let mut d = Done::new("revert_redeem exact", true, RateRule::Restore { rate_restored, nudge }, vec![who]);
                    d.skip_dilution = true;
                    d
                } else {
                    hit(w, "revert_redeem delayed");
                    w.repriced = true;
                    w.promise_slack += VIRTUAL_SHARES / shares + U256::one();
                    let mut d = Done::new("revert_redeem delayed", true, RateRule::Revert { shares, rate_at_begin }, vec![who]);
                    d.skip_dilution = true;
                    d
                }
            }
        }
        Pending::Claim { who, entry, fails } => {
            w.in_flight -= entry.assets;
            if !fails {
                hit(w, "claim paid");
                w.value_out += entry.assets;
                Done::new("claim paid", false, RateRule::Same, none)
            } else {
                let (holdings, unbonding, d) = (w.s.holdings, w.s.unbonding_total, w.s.distributable(now));
                w.s.restore_unbond(who, entry.clone());
                prop_assert!(w.s.unbonds_of(&who).contains(&entry), "{ctx}: entry not restored");
                check_eq!(w.s.holdings, holdings + entry.assets, "{ctx}: holdings after restore_unbond");
                check_eq!(w.s.unbonding_total, unbonding + entry.assets, "{ctx}: unbonding_total after restore_unbond");
                check_eq!(w.s.distributable(now), d, "{ctx}: restore_unbond moved distributable");
                if exact {
                    hit(w, "restore_unbond exact");
                    let after = snap(&w.s);
                    prop_assert!(after == q.before, "{ctx}: restore_unbond did not restore the state\n before: {:#?}\n after: {after:#?}", q.before);
                } else {
                    hit(w, "restore_unbond delayed");
                }
                Done::new("restore_unbond", true, RateRule::Same, none)
            }
        }
        Pending::Fees { amount, fails } => {
            w.in_flight -= amount;
            if !fails {
                hit(w, "fees paid");
                w.value_out += amount;
                Done::new("fees paid", false, RateRule::Same, none)
            } else {
                let (holdings, fees, d) = (w.s.holdings, w.s.fees_accrued, w.s.distributable(now));
                w.s.restore_fees(amount);
                check_eq!(w.s.holdings, holdings + amount, "{ctx}: holdings after restore_fees");
                check_eq!(w.s.fees_accrued, fees + amount, "{ctx}: fees after restore_fees");
                check_eq!(w.s.distributable(now), d, "{ctx}: restore_fees moved distributable");
                if exact {
                    hit(w, "restore_fees exact");
                    let after = snap(&w.s);
                    prop_assert!(after == q.before, "{ctx}: restore_fees did not restore the state\n before: {:#?}\n after: {after:#?}", q.before);
                } else {
                    hit(w, "restore_fees delayed");
                }
                Done::new("restore_fees", true, RateRule::Same, none)
            }
        }
    })
}

enum Action<'a> {
    Op(&'a Op),
    Fire(Queued),
}

/// Run one step (an operation or a completion) and check every generic property around it.
fn execute(w: &mut World, action: Action, probe: u128, ctx: &str) -> Check {
    let before = snap(&w.s);
    let rate_before = w.s.rate_at(w.now);
    let claims_before = claims(&w.s, w.now);

    let done = match action {
        Action::Op(op) => apply(w, op, &before, ctx)?,
        Action::Fire(q) => fire(w, q, ctx)?,
    };
    let ctx = format!("{ctx} [{}]", done.label);
    let now = w.now;
    let rate_after = w.s.rate_at(now);
    if done.changed && done.label != "advance" {
        w.mutations += 1;
    }

    // Errors and completed payouts leave no trace (atomicity).
    if !done.changed {
        let after = snap(&w.s);
        prop_assert!(after == before, "{ctx}: state changed although it should not have\n before: {before:#?}\n after: {after:#?}");
    }

    // Property 3 (and 8): the rate never falls, and only certain ops may move it.
    match done.rate {
        RateRule::Same => check_eq!(rate_after, rate_before, "{ctx}: rate moved"),
        RateRule::Up => prop_assert!(rate_after >= rate_before, "{ctx}: rate fell from {rate_before} to {rate_after}"),
        RateRule::Empty => {
            prop_assert!(rate_after >= rate_before, "{ctx}: emptying the vault lowered the rate from {rate_before} to {rate_after}");
            prop_assert!(rate_after - rate_before <= empty_tick(), "{ctx}: emptying the vault moved the rate by {} > {}", rate_after - rate_before, empty_tick());
        }
        RateRule::Unbond { left } => {
            prop_assert!(rate_after >= rate_before, "{ctx}: unbond lowered the rate from {rate_before} to {rate_after}");
            let tick = if left.is_zero() { empty_tick() } else { SCALE / (left + VIRTUAL_SHARES) + U256::one() };
            prop_assert!(rate_after - rate_before <= tick, "{ctx}: unbond moved the rate by {} > one rounding tick {tick}", rate_after - rate_before);
        }
        RateRule::Restore { rate_restored, nudge } => {
            prop_assert!(rate_after >= rate_restored && rate_after - rate_restored <= nudge, "{ctx}: exact revert left the rate at {rate_after}, expected {rate_restored} (+ at most {nudge})");
        }
        RateRule::Revert { shares, rate_at_begin } => {
            let slack = SCALE / shares + U256::from(3u64);
            let floor = rate_before.min(rate_at_begin);
            prop_assert!(rate_after + slack >= floor, "{ctx}: delayed revert dropped the rate to {rate_after}, below min(before {rate_before}, at begin {rate_at_begin}) - slack {slack}");
        }
    }

    // No dilution: whoever did not take part keeps at least their claim.
    if !done.skip_dilution {
        for (who, claim) in &claims_before {
            if done.actors.contains(who) { continue; }
            let after = w.s.assets_for(w.s.balance_of(who), now).expect("assets_for overflow");
            prop_assert!(after >= *claim, "{ctx}: bystander {who:?} diluted from {claim} to {after}");
        }
    }

    if w.s.total_shares.is_zero() {
        w.promise_slack = U256::zero();
    }
    invariants(w, &ctx)?;
    if let Some(label) = probes(w, probe, &ctx)? {
        hit(w, label);
    }
    Ok(())
}

/// Fire the completions whose wait is over, oldest first, one at a time (each stays in the
/// queue, and in the in-flight bookkeeping, until its own turn), then age the rest.
fn complete_due(w: &mut World, probe: u128, ctx: &str) -> Check {
    let ready = w.queue.iter().filter(|q| q.after == 0).count();
    for _ in 0..ready {
        let pos = w.queue.iter().position(|q| q.after == 0).expect("ready completion");
        let q = w.queue.remove(pos);
        let ctx = format!("{ctx} completion of {:?} at t+{}ms", q.pending, w.now - T0);
        execute(w, Action::Fire(q), probe, &ctx)?;
    }
    for q in &mut w.queue {
        q.after -= 1;
    }
    Ok(())
}

fn run(cfg: &Cfg, steps: &[Step]) -> Result<BTreeMap<&'static str, u32>, TestCaseError> {
    let s = VaultState::new(a(1), a(UNDERLYING), "Vale kUSDC".into(), "kUSDC".into(), 6, cfg.fee_bps, cfg.unbond_secs, cfg.vesting_secs, u(cfg.initial_rate));
    check_eq!(s.instant_fee_bps, cfg.fee_bps.min(MAX_FEE_BPS), "constructor did not clamp the fee");
    check_eq!(s.unbond_period_ms, cfg.unbond_secs.min(MAX_UNBOND_SECS) * 1000, "constructor did not clamp the unbond period");
    check_eq!(s.vesting_period_ms, cfg.vesting_secs.clamp(MIN_VESTING_SECS, MAX_VESTING_SECS) * 1000, "constructor did not clamp the vesting period");
    check_eq!(s.base_rate, u(cfg.initial_rate).max(SCALE), "constructor did not clamp the initial rate");
    let mut w = World {
        s,
        now: T0,
        value_in: U256::zero(),
        value_out: U256::zero(),
        in_flight: U256::zero(),
        mutations: 0,
        repriced: false,
        promise_slack: U256::zero(),
        queue: Vec::new(),
        hits: BTreeMap::new(),
    };
    invariants(&w, "initial")?;

    for (i, step) in steps.iter().enumerate() {
        let ctx = format!("step {i} {:?} at t+{}ms", step.op, w.now - T0);
        execute(&mut w, Action::Op(&step.op), step.probe, &ctx)?;
        complete_due(&mut w, step.probe, &format!("step {i}"))?;
    }
    // Every token call eventually replies: drain what is still in flight.
    let probe = steps.last().map(|s| s.probe).unwrap_or(MIN);
    while !w.queue.is_empty() {
        complete_due(&mut w, probe, "drain")?;
    }
    check_eq!(w.in_flight, U256::zero(), "in-flight value left after draining");
    check_eq!(w.s.fees_pending, U256::zero(), "pending fees left after draining");
    Ok(w.hits)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, max_shrink_iters: 20_000, .. ProptestConfig::default() })]

    #[test]
    fn vault_state_machine(cfg in cfg(), steps in prop::collection::vec(step(), 1..=60)) {
        run(&cfg, &steps)?;
    }
}

/// Guards the sequence test against vacuity: over a fixed batch of random sequences every
/// state-changing operation, every completion and every compensation path (immediate and
/// interleaved) must have run at least once.
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
        "check_deposit", "settle_deposit", "begin_redeem", "redeem paid", "revert_redeem exact", "revert_redeem delayed", "fund",
        "request_unbond", "take_unbond", "claim paid", "restore_unbond exact", "restore_unbond delayed", "take_fees", "fees paid",
        "restore_fees exact", "restore_fees delayed", "advance", "transfer", "propose_session", "accept_session", "revoke_session",
        "set_config", "pause/resume", "record_unconfirmed", "take_unconfirmed",
    ];
    let missing: Vec<_> = expected.iter().filter(|k| total.get(*k).copied().unwrap_or(0) == 0).collect();
    assert!(missing.is_empty(), "operations never exercised: {missing:?}; coverage {total:?}");
}
