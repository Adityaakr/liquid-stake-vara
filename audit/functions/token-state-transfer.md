## `TokenState::transfer` in programs/demo-token/app/src/lib.rs (L97-L103)

**Purpose:** The pure balance move. Both exported VFT commands (`Vft::transfer` L199, `Vft::transfer_from` L210) reduce to it; it carries the pause gate, the zero checks, and the balance check for every token movement the vault performs.

**Inputs & Assumptions:**
- `from` (ActorId): payer. Trust: semi-trusted. In `Vft::transfer` it is `message_source()` (L198); in `Vft::transfer_from` it is caller-supplied and gated by `spend_allowance` unless `spender == from` (L209). Not checked against zero here.
- `to` (ActorId): payee. Trust: untrusted (caller-supplied). Rejected if zero (L99). `to == from` is accepted.
- `value` (U256): amount. Trust: untrusted. Rejected if zero (L100).
- Implicit: `paused` via `ensure_live` (L98), `balances`.
- Preconditions: none beyond what the function checks itself.

**Outputs & Effects:**
- `Err(Paused)` (L98), `Err(ZeroAddress)` (L99), `Err(ZeroAmount)` (L100), `Err(InsufficientBalance)` from `debit` (L101) — none of these write.
- Success: `balances[from] -= value` (L101), `balances[to] += value` (L102). `total_supply` untouched. No events (the exporting service emits them).
- `Err(Overflow)` from `credit` (L102) is reachable only if sum(balances) > total_supply already; under the supply invariant it is not. On that path the `debit` write at L101 has already landed in the `RefCell`; whether it persists depends on how the exported caller fails (see token-vft-transfer.md).

**Block-by-Block:**

```rust
// L98-L100
self.ensure_live()?;
if to.is_zero() { return Err(TokenError::ZeroAddress); }
if value.is_zero() { return Err(TokenError::ZeroAmount); }
```
- **What:** pause gate and argument checks.
- **Why here:** before any write.
- **Assumes:** `paused` is the only pause signal (see token-state-ensure-live.md).
- **Establishes:** not paused; `to != 0`; `value > 0`.
- **Depended on by:** the vault's `pull_underlying`/`push_underlying`, which treat `Paused` as a `TokenRejected` outcome (vault lib.rs L718, L728).

```rust
// L101-L102
self.debit(from, value)?;
self.credit(to, value)
```
- **What:** move the balance.
- **Why here:** debit first so a failing transfer has no partial effect.
- **Assumes:** `from` has at least `value` (checked by `debit` L92).
- **Establishes:** conservation — sum(balances) unchanged. With `from == to`, `debit` removes or reduces then `credit` restores; net zero change (L93, L86).
- **Depended on by:** every VFT-standard consumer that reads `balance_of` after a transfer, including the vault's accounting (the vault does not read the token balance after `transfer_from`; it trusts the `Ok(true)` reply, vault lib.rs L717).

**Cross-Function Dependencies:**
- Callee `ensure_live` (internal, L79-L81): establishes not paused on the only success path.
- Callee `debit` (internal, L90-L95): establishes sufficient balance on the success path; errors before writing.
- Callee `credit` (internal, L83-L88): unreachable failure under the supply invariant.
- Callers: `Vft::transfer` L199, `Vft::transfer_from` L210.
- Shared state: `balances` (also `mint` via credit, `burn` via debit); `paused`.
- Invariant couplings: pause blocks transfers in both directions for the vault; a paused underlying makes `Vault::deposit` fail before any vault state change (vault lib.rs L747) and makes `Vault::redeem` take its revert branch (vault lib.rs L779-L783).

**Open Questions:**
- None.
