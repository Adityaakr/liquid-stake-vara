# panic-dos + error-handling: notes and cleared suspicions

Targets: `programs/vault/app/src/lib.rs`, `programs/vara-pool/app/src/lib.rs`,
`programs/demo-token/app/src/lib.rs` (sails-rs 1.0.1, no_std wasm).

## Profile gate

No `overflow-checks` in any `Cargo.toml` (workspace or generated wasm manifests under
`target/release/build/*/out/Cargo.toml`). Native `u64`/`u128` `+ - *` wrap silently in
the release wasm. `U256` (`uint-0.9.5`) `+ - *` panic unconditionally
(`panic_on_overflow!` at `uint.rs:351`); `/` panics on zero (`uint.rs:943`).

## Cleared: guarded U256 subtractions and divisions

- `begin_redeem` `holdings -= p.net` (vault:408) / `unstake` `reserve -= p.net` (pool:381):
  guarded by `p.assets > available` where `available = distributable <= holdings/reserve`
  (saturating chain) and `p.net <= p.assets` as long as `instant_fee_bps <= 10_000`
  (see panic-1 for the constructor hole).
- `take_unbond` `holdings -= entry.assets` (vault:469) / `reserve -= entry.assets`
  (pool:442): guarded by `list[idx].assets > holdings/reserve` immediately before.
- `take_fees` `holdings -= amount` (vault:488) and pool `collect_fees` `reserve -= amount`
  (pool:837): guarded by `amount > holdings/reserve`.
- `rate_at`, `shares_for`, `assets_for`: `/ total_shares` and `/ distributable` are behind
  `empty()`; `/ base_rate` has `base_rate >= SCALE`; `/ SCALE` constant.
- `locked_rewards`: `vest_end_ms - vest_start_ms` and `vest_end_ms - now_ms` (native u64,
  release-silent wrap) only run when `vest_start_ms < now_ms < vest_end_ms`, so both are
  positive and `span` is non-zero. `fund` sets `vest_start = now`, `vest_end = now +
  time_left` with `time_left >= 1`, so `start < end` always holds.
- `apy_bps`: divisor `span * d` with `d != 0` checked and `span >= 1` per above;
  `bps.low_u32()` behind a `> u32::MAX` clamp.
- `fund`: `/ total` where `total = remaining + assets` and `assets != 0`;
  `time_left.low_u64()` is safe: `time_left` is a weighted mean of `left` (<= u64) and
  `period` (<= 365 days in ms), so it fits u64. `saturating_mul` inputs are bounded by
  u128 amounts x u64 ms, well under 2^256.
- `as_value` (pool:666): `low_u128` behind a `> u128::MAX` clamp; amounts derive from u128
  message values so the clamp never fires.
- `info()`: `/ 1000` constants.

## Cleared: RefCell

Every `emit_vft_transfer` call and every `emit_event` sits outside the `borrow()` /
`borrow_mut()` scope in all three programs (`deposit` 746->754, `redeem` 770->771,
`request_unbond`, pool `stake` 689->691, `unstake`, demo-token `mint`/`burn`/`claim`).
`Vft::new(state).expose(..)` and sails' `emit_event` do not touch the cell. No borrow is
held across an `.await` (the async handlers scope every borrow in a block before the
await). No nested-borrow path found.

## Cleared: indexing, Vec removal

`take_unbond`: `idx` comes from `position()` on the same list and `list.remove(idx)`
re-fetches the same (unchanged) Vec via `get_mut`; in-bounds. `restore_unbond` sorts by id
after pushing, fine.

## Cleared: error variant mapping / `?` after await

- `burn_shares` maps `debit` errors to `InsufficientShares` deliberately; the
  `total_shares.checked_sub(..).ok_or(Overflow)` is really an underflow but only cosmetic.
- `actor_for` returns `SessionExpired` when `session_keys` has the key but `sessions`
  lacks the owner; the two maps are kept consistent by `accept_session`/`revoke_session`
  so the branch is dead.
- Post-await `?` in `deposit`/`fund_rewards` (`settle_deposit`/`fund` return `Err` only on
  U256 overflow): returning `Err` does not revert the segment, so `sweep_orphans` would
  persist while the deposit is not credited, and the pulled tokens are stranded (same
  consequence as panic-5). Unreachable amounts; recorded for completeness.
- Pool `collect_fees` error path sets `fees_accrued = amount` (overwrite, not add). Safe
  because the pool is synchronous: `fees_accrued` was zeroed in the same execution.

## Weak suspicions (not filed)

- **State growth**: `unbonds` per owner (each `request_unbond` needs only enough shares for
  1 base unit), `allowances` (any spender), `balances` (dust transfers). Standard
  ERC-20-shaped growth paid per message; `take_unbond` is O(n) and `restore_unbond`
  O(n log n) but only over the caller's own list (self-DoS). Larger state raises
  page-load gas for every message; no cap on entries. Consider capping unbond entries per
  owner (e.g. 32) and rejecting dust unbonds.
- **`next_unbond_id += 1`** (native u64, release-silent wrap): 2^64 unbonds, unreachable.
- **`emit_vft_transfer` in demo-token `Faucet::claim`** after the borrow block, fine; the
  faucet is rate-limited per account only, so `mint` growth of `last_claim` is one entry
  per claimer (fine).
- **Gas budget of the wake segment** (`GAS_ONE_CALL`, `TOKEN_CALL_GAS`): if the reserved
  gas is not enough for re-instantiation plus `settle_deposit` plus two event sends, the
  wake traps and deposits strand (panic-5). Out of scope for this cluster; hand to the
  gas/async worker for measurement on a 1.10 node.
- **`pay_out` to a user below the existential deposit** (pool:672): uncertain runtime
  behaviour; covered as a sub-case of panic-4.
- **demo-token**: no arithmetic beyond `checked_*`/`saturating_*`; `faucet_claim` uses
  saturating cooldown math; nothing to report in this cluster.
