## `Admin::revoke_minter` in programs/demo-token/app/src/lib.rs (L326-L336)

**Purpose:** Exported `RevokeMinter(account) -> bool throws TokenError` (IDL L90). Removes `account` from `minters`.

**Inputs & Assumptions:**
- `account` (ActorId). Trust: untrusted argument from the admin. Absence is not an error (L332 `remove` ignores missing keys).
- Implicit: `caller = message_source()` (L328); `admin`.
- Preconditions: caller is admin — `ensure_admin` L331. Not gated by pause.

**Outputs & Effects:**
- Success: `minters.remove(&account)` (L332), `MinterRevoked { account }` (L334), reply `true` (L335) — emitted even when nothing was removed.
- Error: `Unauthorized` (L331), structured panic (L326).

**Block-by-Block:**

```rust
// L329-L333
{
    let mut s = self.state.borrow_mut();
    s.ensure_admin(caller)?;
    s.minters.remove(&account);
}
```
- **What:** authorize, remove.
- **Why here:** single write after the check.
- **Assumes:** nothing.
- **Establishes:** `!minters.contains(account)`. If `account == admin`, `ensure_minter(admin)` still succeeds via the `admin == who` arm (L160); revocation has no effect on the admin's authority.
- **Depended on by:** subsequent `Admin::mint`/`burn` by `account` failing with `Unauthorized` (tests/gtest.rs L131-L133).

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event`.
- Callers: admin only.
- Shared state: `minters` (also `grant_minter` L320).
- Invariant couplings: revocation takes effect for the next message; there is no in-flight interleaving because handlers are synchronous.

**Open Questions:**
- None.
