## `TokenState::debit` in programs/demo-token/app/src/lib.rs (L90-L95)

**Purpose:** Subtracts `value` from `from`'s balance, failing with `InsufficientBalance` if it would go negative, and prunes the map entry when the result is zero. It is the balance check for `transfer` (L101) and `burn` (L133).

**Inputs & Assumptions:**
- `from` (ActorId): account debited. Trust: semi-trusted — in `transfer` it is the message source or an allowance-checked owner; in `burn` it is an arbitrary admin/minter-supplied address (L302, L307). No zero check here or in callers.
- `value` (U256): amount. Trust: semi-trusted — callers reject zero (L100, L132). With `value == 0` and `bal == 0`, `debit` would remove an absent key and return `Ok` (L92-L93); no caller reaches that.
- Implicit: reads `balances` via `balance_of` (L91).

**Outputs & Effects:**
- `Err(InsufficientBalance)` and no write when `bal < value` (L92).
- Otherwise writes `balances[from] = bal - value`, or removes the key when the remainder is zero (L93).

**Block-by-Block:**

```rust
// L91-L93
let bal = self.balance_of(&from);
let new = bal.checked_sub(value).ok_or(TokenError::InsufficientBalance)?;
if new.is_zero() { self.balances.remove(&from); } else { self.balances.insert(from, new); }
```
- **What:** checked subtraction then write-or-prune.
- **Why here:** In `transfer` it runs before `credit` (L101-L102) so a failing transfer writes nothing; in `burn` before the supply decrement (L133-L134).
- **Assumes:** nothing beyond map semantics.
- **Establishes:** `balances[from] == old - value` and `old >= value`; no zero-valued entries for `from`.
- **Depended on by:** `credit` in `transfer` (overflow unreachable, see token-state-credit.md); `burn`'s supply decrement (cannot underflow while sum(balances) == total_supply).

**Cross-Function Dependencies:**
- Callee `balance_of` (internal, L71-L73).
- Callers: `transfer` L101, `burn` L133.
- Shared state: `balances` also written by `credit` L86.
- Invariant couplings: pruning at L93 means "absent key" and "zero balance" are the same observable state through `balance_of` (L72); nothing in the program distinguishes them.

**Open Questions:**
- None.
