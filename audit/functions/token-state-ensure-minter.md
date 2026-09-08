## `TokenState::ensure_minter` in programs/demo-token/app/src/lib.rs (L159-L161)

**Purpose:** The supply-change gate: `Admin::mint` L292 and `Admin::burn` L306. The admin is implicitly a minter.

**Inputs & Assumptions:**
- `who` (ActorId): `message_source()` at both call sites (L289, L303). Trust: trusted identity.
- Implicit: `admin`, `minters` (L160).

**Outputs & Effects:**
- `Ok(())` iff `who == admin` or `minters.contains(who)` (L160). No writes.

**Block-by-Block:**

```rust
// L160
if self.admin == who || self.minters.contains(&who) { Ok(()) } else { Err(TokenError::Unauthorized) }
```
- **What:** two-way membership test.
- **Why here:** before `mint`/`burn` inside the borrow block.
- **Assumes:** `minters` only changes through `grant_minter` L320 / `revoke_minter` L332 — true by grep. Membership is not affected by pause.
- **Establishes:** caller holds mint/burn authority. The admin cannot be stripped of it by `revoke_minter` (the `admin == who` arm is independent of the set).
- **Depended on by:** `Admin::mint`, `Admin::burn`. Mirrored by the query `is_minter` (L402).

**Cross-Function Dependencies:**
- No callees. Callers: L292, L306. Shared state: `admin`, `minters`.
- Invariant couplings: the vault program is expected to hold this role so it can "mint yield" (module docs L5); the grant itself happens off-chain via `grant_minter`. A minter's authority extends to `burn` from any address (L302-L307).

**Open Questions:**
- None. (Checked: programs/vault/app/src/lib.rs calls only `vft().transfer_from` L716 and `vft().transfer` L726; it never calls the token's `Admin` service, so the vault does not need the minter role for any code path in the repo. The module docs' "so the vault can mint yield" (L5) describes an off-chain arrangement, not a call in the vault program.)
