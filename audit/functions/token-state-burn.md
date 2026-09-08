## `TokenState::burn` in programs/demo-token/app/src/lib.rs (L130-L136)

**Purpose:** Destroys `value` from `from`'s balance and shrinks `total_supply`. Only reachable through `Admin::burn` (L307) after `ensure_minter` (L306).

**Inputs & Assumptions:**
- `from` (ActorId): account burned. Trust: untrusted argument of an authorized caller (L302). Not checked against zero; not checked against the caller; no allowance consulted. Established by: nothing in `burn` (L130-L136) or `Admin::burn` (L302-L312) compares `from` to the caller or reads `allowances`.
- `value` (U256): amount. Trust: untrusted. Zero rejected (L132).
- Implicit: `paused` (L131), `balances`, `total_supply`.

**Outputs & Effects:**
- `Err(Paused)` (L131), `Err(ZeroAmount)` (L132), `Err(InsufficientBalance)` from `debit` (L133): no writes.
- Success: `balances[from] -= value` (L133), `total_supply -= value` (L134).
- `Err(Overflow)` at L134 (name for a supply underflow) after the debit has already landed in the `RefCell`. Unreachable under sum(balances) == total_supply, since `debit` succeeded on `value <= balances[from] <= total_supply`.

**Block-by-Block:**

```rust
// L133-L134
self.debit(from, value)?;
self.total_supply = self.total_supply.checked_sub(value).ok_or(TokenError::Overflow)?;
```
- **What:** debit then decrement supply.
- **Why here:** debit is the balance check; the supply decrement is guarded only by the invariant.
- **Assumes:** sum(balances) == total_supply on entry. Established by: `mint` L124-L126, `burn` itself, `transfer` conservation L101-L102, and `Program::new` starting both at zero (L481). No other writer of `balances` or `total_supply` exists in the file.
- **Establishes:** invariant preserved on the success path.
- **Depended on by:** `Admin::burn`'s events (L309-L310) reporting `from`/`value`.

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L79-L81).
- Callee `debit` (internal, L90-L95).
- Callers: `Admin::burn` L307.
- Shared state: `balances`, `total_supply`.
- Invariant couplings: a minter can burn from any address, including the vault program's own balance of the underlying, without the vault's participation. The vault treats its token balance as its holdings without reading it back (vault lib.rs L714-L732).

**Open Questions:**
- None.
