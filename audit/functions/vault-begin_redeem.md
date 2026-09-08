## `VaultState::begin_redeem` in programs/vault/app/src/lib.rs (L398-L410)

**Purpose:** The pre-await half of an instant exit: validate, burn the shares, credit the fee, and take the net payout out of `holdings` *before* the outbound transfer (doc rule 2 at L16-L17), so that a message interleaved during the await cannot price off assets already promised (comment L396-L397, test L1106-L1114).

**Inputs & Assumptions:**
- `owner` (ActorId): resolved by `actor_for` (L766). Trust: semi-trusted.
- `shares` (U256): from the payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Implicit state read: `paused`, `balances[owner]`, `instant_fee_bps`, pricing state.
- Precondition: `instant_fee_bps <= BPS` for `preview_redeem` L297 (unchecked `assets - fee`). Established by `set_config` L506 (`<= 1_000`); at construction L220, nothing found.

**Outputs & Effects:**
- State writes (all after the last possible `Err`): `balances[owner]`, `total_shares`, possibly `base_rate` (via `burn_shares` L406); `fees_accrued += p.fee` L407; `holdings -= p.net` L408.
- Returns `Ok(RedeemPreview{assets, fee, net})` or one of `Paused` L399, `ZeroAmount` L400/L403, `InsufficientShares` L401 (and L406), `Overflow` (L402 via `assets_for`, L406), `InsufficientReserve{available}` L405.
- No partial writes: every `Err` is returned before L406, except the `Overflow` at L361 inside `burn_shares`, which is unreachable while `sum(balances) == total_shares`.

**Block-by-Block:**

```rust
// L399-L401
self.ensure_live()?;
if shares.is_zero() { return Err(VaultError::ZeroAmount); }
if self.balance_of(&owner) < shares { return Err(VaultError::InsufficientShares); }
```
- **What:** Pause gate, zero gate, balance gate.
- **Why here:** All before any pricing or write; `Vault::redeem` has taken a `borrow_mut` (L764) so the checks and writes are one atomic segment.
- **Establishes:** `shares <= balance_of(owner) <= total_shares`.
- **Depended on by:** L402 (`assets_for` bound), L406.

```rust
// L402-L403
let p = self.preview_redeem(shares, now_ms)?;
if p.net.is_zero() { return Err(VaultError::ZeroAmount); }
```
- **What:** Price the exit and refuse a zero net payout.
- **Why here:** The token rejects a zero-value transfer (demo `transfer` L100 → `ZeroAmount` → trap); refusing here avoids burning shares for a transfer that would fail and then be reverted.
- **Assumes:** `preview_redeem` does not panic (fee bound above).
- **Establishes:** `p.net >= 1`, hence `p.assets >= 1`.
- **Depended on by:** L408, `push_underlying` L772.

```rust
// L404-L405
let available = self.distributable(now_ms);
if p.assets > available { return Err(VaultError::InsufficientReserve { available }); }
```
- **What:** Reserve check against `distributable`, using the *gross* assets (fee included).
- **Why here:** Makes L408's unchecked `holdings -= p.net` safe: `p.net <= p.assets <= distributable <= holdings`.
- **Assumes:** nothing — with `shares <= total_shares` the non-empty branch of `assets_for` already gives `p.assets <= distributable`; this check is the one that matters on the `empty` branch (`distributable == 0` with shares outstanding), where `assets_for` quotes `shares * base_rate / SCALE` regardless of reserves. On that branch `available == 0` and every non-zero payout is refused.
- **Establishes:** `p.assets <= distributable(now_ms)`.
- **Depended on by:** L408 (no underflow); the accounting bound in `vault-distributable.md`.

```rust
// L406-L408
self.burn_shares(owner, shares, now_ms)?;
self.fees_accrued = self.fees_accrued.saturating_add(p.fee);
self.holdings -= p.net;
```
- **What:** Burn, book the fee, take the payout out.
- **Why here:** Burn before the holdings change so `burn_shares`'s rate snapshot (L359) sees the pre-exit state. Fee stays inside `holdings` (only `net` leaves), which is why `fees_accrued` is a claim on `holdings` and `take_fees` L486 checks against it.
- **Assumes:** `U256::-=` panics on underflow (primitive-types); the reserve check at L405 rules it out.
- **Establishes:** after this line `distributable` is exactly `old_distributable - p.assets`; the remaining holders' rate is unchanged up to rounding (test L1114).
- **Depended on by:** `Vault::redeem` L771-L782; `revert_redeem` on failure.

**Cross-Function Dependencies:**
- Callee `preview_redeem` (internal L294-L298): depends on `assets_for` and the fee bound. Callee `burn_shares`, `distributable`, `ensure_live`: records above.
- Callers: `Vault::redeem` L768 only.
- Shared state: `holdings`, `fees_accrued`, `balances`, `total_shares`, `base_rate`.
- Invariant couplings: `holdings >= fees + unbonding + locked` preserved (see `vault-distributable.md`). The compensating write is `revert_redeem` L413-L417; between the two, `collect_fees`, `deposit`, `fund_rewards`, `request_unbond`, another `redeem`, `pause`, `set_config`, session changes and share transfers can all run (see `vault-Vault-redeem.md`).

**Open Questions:**
- none beyond the constructor fee bound.
