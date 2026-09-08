## `Admin::grant_minter` in programs/demo-token/app/src/lib.rs (L314-L324)

**Purpose:** Exported `GrantMinter(account) -> bool throws TokenError` (IDL L80). Adds `account` to `minters`, giving it `Admin::mint` and `Admin::burn` authority.

**Inputs & Assumptions:**
- `account` (ActorId). Trust: untrusted argument from the admin. Not checked against zero, against the admin, or against existing membership (L320 is a plain `BTreeSet::insert`).
- Implicit: `caller = message_source()` (L316); `admin`.
- Preconditions: caller is admin — `ensure_admin` L319. Not gated by pause.

**Outputs & Effects:**
- Success: `minters.insert(account)` (L320), `MinterGranted { account }` (L322), reply `true` (L323). Re-granting an existing minter is a no-op write and still emits.
- Error: `Unauthorized` (L319), structured panic (L314).

**Block-by-Block:**

```rust
// L317-L321
{
    let mut s = self.state.borrow_mut();
    s.ensure_admin(caller)?;
    s.minters.insert(account);
}
```
- **What:** authorize, insert.
- **Why here:** single write after the check.
- **Assumes:** nothing about `account`.
- **Establishes:** `minters.contains(account)`; `ensure_minter(account)` succeeds from now on (L160).
- **Depended on by:** `Admin::mint`/`Admin::burn` by `account`; `is_minter`/`minters` queries (L395-L403).

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event` (external-source-available).
- Callers: admin only (tests/gtest.rs L118-L121).
- Shared state: `minters` also written by `revoke_minter` L332.
- Invariant couplings: the set is unbounded; there is no cap on the number of minters. Works while paused.

**Open Questions:**
- None.
