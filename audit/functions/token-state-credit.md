## `TokenState::credit` in programs/demo-token/app/src/lib.rs (L83-L88)

**Purpose:** Adds `value` to `to`'s balance with overflow detection. It is the only place a balance grows; `transfer` (L102) and `mint` (L125) rest on it.

**Inputs & Assumptions:**
- `to` (ActorId): recipient. Trust: semi-trusted — callers validate it against zero before calling (`transfer` L99, `mint` L122); `credit` itself does not.
- `value` (U256): amount. Trust: semi-trusted — callers reject zero (L100, L123); `credit` accepts zero and would insert an explicit zero-balance entry (L86).
- Implicit: reads `balances` via `balance_of` (L72), which returns 0 for absent keys.
- Preconditions: none enforced here.

**Outputs & Effects:**
- On success inserts `bal + value` at `balances[to]` (L86). On overflow returns `Err(TokenError::Overflow)` with no write (L85).

**Block-by-Block:**

```rust
// L84-L86
let bal = self.balance_of(&to);
let new = bal.checked_add(value).ok_or(TokenError::Overflow)?;
self.balances.insert(to, new);
```
- **What:** read-add-write with checked addition.
- **Why here:** The check precedes the insert, so no partial write exists on the error path.
- **Assumes:** For the overflow branch to be unreachable, `bal + value <= total_supply' <= U256::MAX`. In `transfer` this holds because `debit` (L101) already succeeded on the same `value` and the sum of balances equals `total_supply` (see couplings). In `mint` it holds because `total_supply.checked_add(value)` succeeded first (L124). Established by: L101/L124 ordering plus the supply invariant.
- **Establishes:** `balances[to] == old + value`.
- **Depended on by:** `transfer` L102 (final step), `mint` L125 (before the supply write at L126).

**Cross-Function Dependencies:**
- Callee `balance_of` (internal, L71-L73): returns stored value or zero. No failure path.
- Callers: `transfer` L102, `mint` L125.
- Shared state: `balances` also written by `debit` (L93).
- Invariant couplings: sum(balances) == total_supply. `credit` alone breaks it; each caller pairs it with a `debit` (transfer) or a `total_supply` increment (mint).

**Open Questions:**
- None.
