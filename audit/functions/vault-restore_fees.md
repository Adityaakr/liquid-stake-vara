## `VaultState::restore_fees` in programs/vault/app/src/lib.rs (L492-L495)

**Purpose:** Compensation for `take_fees` after a failed transfer: add the amount back to both `fees_accrued` and `holdings`.

**Inputs & Assumptions:**
- `amount` (U256): what `take_fees` returned, captured pre-await (L904). Trust: internal.
- Precondition: the token did not move `amount` to `to`. Same caveat as `revert_redeem` for the `Timeout` arm.

**Outputs & Effects:**
- State writes: `fees_accrued += amount` L493, `holdings += amount` L494 (saturating). Cannot fail.

**Block-by-Block:**

```rust
// L493-L494
self.fees_accrued = self.fees_accrued.saturating_add(amount);
self.holdings = self.holdings.saturating_add(amount);
```
- **What:** Additive restore, so fees credited by redeems during the await are kept.
- **Assumes:** nothing else; there is no gate on admin or pause here.
- **Establishes:** state as before `take_fees` plus whatever accrued in the window.
- **Depended on by:** a later `collect_fees`.

**Cross-Function Dependencies:**
- Callers: `Vault::collect_fees` L913 only.
- Shared state: as `take_fees`.
- Invariant couplings: accounting bound preserved.

**Open Questions:**
- same timeout question as `vault-revert_redeem.md`.
