## `TokenState::mint` in programs/demo-token/app/src/lib.rs (L120-L128)

**Purpose:** Creates `value` new tokens for `to` and grows `total_supply` by the same amount. It is the only path that increases supply; `Admin::mint` (L293) and `faucet_claim` (L146) both use it. Authorization is not here — `Admin::mint` checks `ensure_minter` before calling (L292) and `faucet_claim` is deliberately open.

**Inputs & Assumptions:**
- `to` (ActorId): recipient. Trust: untrusted (admin/minter-supplied or the faucet caller). Zero rejected (L122).
- `value` (U256): amount. Trust: semi-trusted — from a minter argument (L288) or `faucet_amount` (L145). Zero rejected (L123).
- Implicit: `paused` (L121), `total_supply`, `balances`.
- Preconditions: caller is authorized — established by `Admin::mint` L292 for the admin path; for the faucet path nothing (by design, L138-L149).

**Outputs & Effects:**
- `Err(Paused)`, `Err(ZeroAddress)`, `Err(ZeroAmount)` (L121-L123), `Err(Overflow)` from the supply check (L124) — no writes on any of these.
- `Err(Overflow)` from `credit` (L125): unreachable when sum(balances) == total_supply, because the supply check at L124 already bounded `bal + value`.
- Success: `balances[to] += value` (L125) then `total_supply += value` (L126).

**Block-by-Block:**

```rust
// L124-L126
let supply = self.total_supply.checked_add(value).ok_or(TokenError::Overflow)?;
self.credit(to, value)?;
self.total_supply = supply;
```
- **What:** compute new supply, credit, commit supply.
- **Why here:** the supply check comes first so the credit cannot be the step that overflows; the supply write is last so an error from `credit` leaves supply untouched.
- **Assumes:** no other writer of `total_supply` between L124 and L126 — true, the function is synchronous and holds the `RefMut`.
- **Establishes:** sum(balances) == total_supply is preserved (both grow by `value`).
- **Depended on by:** the vault's economics only indirectly: the vault program holds a balance of this token and uses `total_supply`-independent logic (vault lib.rs L714-L732 never reads the token's supply).

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L79-L81).
- Callee `credit` (internal, L83-L88).
- Callers: `Admin::mint` L293 (after `ensure_minter` L292), `faucet_claim` L146 (after cooldown checks L139-L144).
- Shared state: `total_supply` also written by `burn` L134; `balances` by `debit`, `credit`.
- Invariant couplings: pause blocks minting (L121), including faucet claims.

**Open Questions:**
- None.
