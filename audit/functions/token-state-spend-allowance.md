## `TokenState::spend_allowance` in programs/demo-token/app/src/lib.rs (L112-L118)

**Purpose:** Consumes `value` from the `(owner, spender)` allowance, treating `U256::MAX` as unlimited. It is the authorization check behind `Vft::transfer_from` (L209) and therefore behind every vault deposit pull.

**Inputs & Assumptions:**
- `owner` (ActorId): the `from` of a `transfer_from`, caller-supplied. Trust: untrusted.
- `spender` (ActorId): `message_source()` at the only call site (L206). Trust: trusted identity.
- `value` (U256): caller-supplied. Trust: untrusted. Zero is not rejected here (L115: `current - 0` succeeds); the subsequent `transfer` rejects it (L100).
- Implicit: `allowances` via `allowance` (L76). Does not read `paused`.

**Outputs & Effects:**
- `current == U256::MAX`: returns `Ok(())` with no write (L114). The allowance is never decremented.
- `current < value`: `Err(InsufficientAllowance)`, no write (L115).
- Otherwise writes `current - value`, or removes the key when the remainder is zero (L116).

**Block-by-Block:**

```rust
// L113-L114
let current = self.allowance(&owner, &spender);
if current == U256::MAX { return Ok(()); }
```
- **What:** unlimited-allowance short circuit.
- **Why here:** before the subtraction so MAX is never reduced.
- **Assumes:** callers intend `U256::MAX` as "infinite"; nothing distinguishes an explicitly granted MAX from any other value. Established by: `approve` L108 (any value including MAX is stored verbatim); unit test L533-L535.
- **Establishes:** for MAX, no state change and no bound on cumulative spend.
- **Depended on by:** `Vft::transfer_from` L209.

```rust
// L115-L116
let rest = current.checked_sub(value).ok_or(TokenError::InsufficientAllowance)?;
if rest.is_zero() { self.allowances.remove(&(owner, spender)); } else { self.allowances.insert((owner, spender), rest); }
```
- **What:** checked decrement with pruning.
- **Why here:** runs before `transfer` (L210), so on the success path the allowance is reduced before the balance moves; on a failed `transfer` this write has already happened inside the `RefCell` and its persistence depends on the exported caller's failure mode (see token-vft-transfer-from.md).
- **Assumes:** `owner` actually holds the balance — not checked here; `transfer`'s `debit` checks it (L101).
- **Establishes:** `allowance' == allowance - value` (non-MAX case).
- **Depended on by:** the VFT allowance semantics consumers rely on (vault deposits reduce the depositor's allowance to the vault by exactly `assets`, vault lib.rs L716).

**Cross-Function Dependencies:**
- Callee `allowance` (internal, L75-L77): default zero for absent key.
- Callers: `Vft::transfer_from` L209 only, and only when `spender != from`.
- Shared state: `allowances` also written by `approve` L108.
- Invariant couplings: not gated by `ensure_live`; pause enforcement for `transfer_from` comes from `transfer` L98 one call later.

**Open Questions:**
- None.
