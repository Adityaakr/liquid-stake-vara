## `TokenState::approve` in programs/demo-token/app/src/lib.rs (L105-L110)

**Purpose:** Sets (overwrites) the `(owner, spender)` allowance. It is how a depositor authorizes the vault before `Vault::deposit` calls `transfer_from` (vault lib.rs L736, L747).

**Inputs & Assumptions:**
- `owner` (ActorId): always `message_source()` at the only call site (L190). Trust: trusted identity.
- `spender` (ActorId): caller-supplied. Trust: untrusted. Rejected if zero (L107).
- `value` (U256): new allowance, not a delta. Trust: untrusted. Zero deletes the entry (L108). `U256::MAX` is the unlimited sentinel consumed by `spend_allowance` (L114).
- Implicit: `paused` (L106).

**Outputs & Effects:**
- `Err(Paused)` (L106), `Err(ZeroAddress)` (L107), no writes.
- Success: `allowances[(owner, spender)] = value`, or key removed when `value == 0` (L108).

**Block-by-Block:**

```rust
// L106-L108
self.ensure_live()?;
if spender.is_zero() { return Err(TokenError::ZeroAddress); }
if value.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), value); }
```
- **What:** gate, validate, overwrite.
- **Why here:** single write, nothing before it can partially apply.
- **Assumes:** nothing about the previous allowance; there is no increase/decrease form and no check that the previous value matches an expectation.
- **Establishes:** `allowance(owner, spender) == value`.
- **Depended on by:** `spend_allowance` L113.

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L79-L81).
- Callers: `Vft::approve` L191.
- Shared state: `allowances` also written by `spend_allowance` L116.
- Invariant couplings: approvals are blocked while paused (L106), so a pause also prevents new vault deposit authorizations, not only transfers. `approve` with `owner == spender` is accepted; such an allowance is never consulted because `transfer_from` skips `spend_allowance` when `spender == from` (L209).

**Open Questions:**
- None.
