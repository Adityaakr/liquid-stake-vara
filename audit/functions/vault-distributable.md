## `VaultState::distributable` in programs/vault/app/src/lib.rs (L250-L252)

**Purpose:** The assets that belong to share holders right now: physical, accounted holdings minus fees owed to the admin, minus assets already promised to unbond entries, minus rewards not yet vested. Every price in the vault (`rate_at`, `shares_for`, `assets_for`) and every reserve check (`begin_redeem` L404, `request_unbond` L448) is computed from it.

**Inputs & Assumptions:**
- `now_ms` (u64): block timestamp. Trust: trusted (see `vault-locked_rewards.md`).
- Implicit state read: `holdings` (L188), `fees_accrued` (L189), `unbonding_total` (L190), and the vesting fields via `locked_rewards`.
- Precondition (semantic, not required for memory safety): `holdings >= fees_accrued + unbonding_total + locked_rewards(now_ms)`. The code does not rely on it: all three subtractions are `saturating_sub` (L251), so a violation silently floors the result at zero instead of underflowing. What establishes the bound, transition by transition:
  - `settle_deposit` L392: `holdings += assets`, nothing else grows — bound preserved.
  - `fund` L435-L438: `holdings += assets` and `vest_amount = remaining + assets`, so `locked` grows by at most `assets` — preserved.
  - `begin_redeem` L404-L408: requires `p.assets <= distributable` (L405), then `holdings -= p.net`, `fees_accrued += p.fee`, with `net + fee == assets` (L297) — preserved.
  - `request_unbond` L448-L451: requires `assets <= distributable` (L449), then `unbonding_total += assets` — preserved.
  - `take_unbond` L468-L469 and `restore_unbond` L475-L476: move the same amount out of / into both `holdings` and `unbonding_total` — preserved.
  - `take_fees` L487-L488 and `restore_fees` L493-L494: same for `fees_accrued` — preserved.
  - `sweep_orphans` L372-L373: sets `fees_accrued += distributable` when `total_shares == 0`, i.e. drives the bound to equality — preserved.
  - `revert_redeem` L415-L416: `fees_accrued.saturating_sub(p.fee)` and `holdings += p.net`. If `collect_fees` ran between `begin_redeem` and this revert (the async window in `Vault::redeem`), `fees_accrued` may already be below `p.fee` and saturates to zero; `holdings` is still consistent with the tokens actually held (the fee was paid out, the net payout was not). Preserved, but the fee that was already collected is then borne by remaining holders (`distributable` is lower by `fee` than before the failed redeem).

**Outputs & Effects:**
- Returns `U256`. No writes, no messages.
- Postcondition: `result <= holdings`.

**Block-by-Block:**

```rust
// L251
self.holdings.saturating_sub(self.fees_accrued).saturating_sub(self.unbonding_total).saturating_sub(self.locked_rewards(now_ms))
```
- **What:** Three saturating subtractions in the fixed order fees, unbonds, locked rewards.
- **Why here:** Single expression; the order only matters when the bound above is violated (then whichever term comes last is the one that gets shorted).
- **Assumes:** `holdings` reflects tokens the vault really holds. `holdings` is internal (comment L186-L187): it counts only what `settle_deposit` and `fund` added after a successful `transfer_from`, and what the outbound flows removed before their `transfer`. It is not reconciled against the token's `balance_of(vault)` anywhere; nothing found.
- **Establishes:** the number every pricing function uses.
- **Depended on by:** `empty` L256, `rate_at` L262, `shares_for` L270, `assets_for` L278, `total_assets` L282, `apy_bps` L287, `begin_redeem` L404, `request_unbond` L448, `sweep_orphans` L372, `info` L570.

**Cross-Function Dependencies:**
- Callee `locked_rewards` (internal, L241-L247): depended on to return a value `<= vest_amount` on every path; it does (see its record).
- Callers: listed above. `begin_redeem` and `request_unbond` use it both as the price base and as the reserve ceiling in the same call with the same `now_ms`, so the payout they compute is by construction `<= distributable`.
- Shared state: `holdings` is written by `settle_deposit` L392, `begin_redeem` L408, `revert_redeem` L416, `fund` L438, `take_unbond` L469, `restore_unbond` L476, `take_fees` L488, `restore_fees` L494. `fees_accrued` by `sweep_orphans` L373, `begin_redeem` L407, `revert_redeem` L415, `take_fees` L487, `restore_fees` L493. `unbonding_total` by `request_unbond` L451, `take_unbond` L468, `restore_unbond` L475.
- Invariant couplings: every async handler takes its payout out of `holdings` *before* its await (`begin_redeem`, `take_unbond`, `take_fees`), so a message interleaved during the await sees a `distributable` that already excludes the in-flight payout (doc rule 2 at L16-L17; test L1114). Inbound flows add to `holdings` only after the await (`settle_deposit`, `fund`), so in-flight deposits are invisible (rule 3 at L18-L19).

**Open Questions:**
- unclear; need to inspect whether any deployment path transfers underlying to the vault outside `deposit`/`fund_rewards` (a plain VFT `transfer` to the vault's address). Such tokens are held but never counted here; the comment at L186-L187 accepts that. Whether anything later reconciles them is not in this file.
